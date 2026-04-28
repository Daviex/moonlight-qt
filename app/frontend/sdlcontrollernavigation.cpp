#include "sdlcontrollernavigation.h"

#include "settings/mappingmanager.h"

#include <utility>

#define AXIS_NAVIGATION_REPEAT_DELAY 150

SdlControllerNavigation::SdlControllerNavigation(StreamingPreferences* prefs, QObject* parent)
    : QObject(parent),
      m_Prefs(prefs),
      m_Sink(nullptr),
      m_Enabled(false),
      m_UiNavMode(false),
      m_FirstPoll(false),
      m_HasFocus(false),
      m_LastAxisNavigationEventTime(0)
{
    m_PollingTimer = new QTimer(this);
    connect(m_PollingTimer, &QTimer::timeout, this, &SdlControllerNavigation::onPollingTimerFired);
}

SdlControllerNavigation::~SdlControllerNavigation()
{
    disable();
}

void SdlControllerNavigation::setSink(IControllerNavigationSink* sink)
{
    m_Sink = sink;
}

void SdlControllerNavigation::enable()
{
    if (m_Enabled) {
        return;
    }

    // We have to initialize and uninitialize this in enable()/disable()
    // because we need to get out of the way of the Session class. If it
    // doesn't get to reinitialize the GC subsystem, it won't get initial
    // arrival events. Additionally, there's a race condition between
    // our QML objects being destroyed and SDL being deinitialized that
    // this solves too.
    if (SDL_InitSubSystem(SDL_INIT_GAMECONTROLLER) != 0) {
        SDL_LogError(SDL_LOG_CATEGORY_APPLICATION,
                     "SDL_InitSubSystem(SDL_INIT_GAMECONTROLLER) failed: %s",
                     SDL_GetError());
        return;
    }

    MappingManager mappingManager;
    mappingManager.applyMappings();

    // Drop all pending gamepad add events. SDL will generate these for us
    // on first init of the GC subsystem. We can't depend on them due to
    // overlapping lifetimes of SdlControllerNavigation instances, so we
    // will attach ourselves.
    //
    // NB: We use SDL_JoystickUpdate() instead of SDL_PumpEvents() because
    // the latter can do a bit more work that we want (like handling video
    // events that we intentionally do not want to process yet).
    SDL_JoystickUpdate();
    SDL_FlushEvent(SDL_CONTROLLERDEVICEADDED);

    // Open all currently attached game controllers
    int numJoysticks = SDL_NumJoysticks();
    for (int i = 0; i < numJoysticks; i++) {
        if (SDL_IsGameController(i)) {
            SDL_GameController* gc = SDL_GameControllerOpen(i);
            if (gc != nullptr) {
                m_Gamepads.append(gc);
            }
        }
    }

    m_Enabled = true;

    // Start the polling timer if the window is focused
    updateTimerState();
}

void SdlControllerNavigation::disable()
{
    if (!m_Enabled) {
        return;
    }

    m_Enabled = false;
    updateTimerState();
    Q_ASSERT(!m_PollingTimer->isActive());

    while (!m_Gamepads.isEmpty()) {
        SDL_GameControllerClose(m_Gamepads[0]);
        m_Gamepads.removeAt(0);
    }

    SDL_QuitSubSystem(SDL_INIT_GAMECONTROLLER);
}

void SdlControllerNavigation::notifyWindowFocus(bool hasFocus)
{
    m_HasFocus = hasFocus;
    updateTimerState();
}

void SdlControllerNavigation::sendAction(ControllerNavigationAction action, bool pressed)
{
    if (m_Sink != nullptr) {
        m_Sink->handleControllerNavigation(action, pressed);
    }
}

void SdlControllerNavigation::sendActionPress(ControllerNavigationAction action)
{
    sendAction(action, true);
    sendAction(action, false);
}

void SdlControllerNavigation::onPollingTimerFired()
{
    SDL_Event event;

    // Update joystick state without pumping other events (see enable() comment)
    SDL_JoystickUpdate();

    // Discard any pending button events on the first poll to avoid picking up
    // stale input data from the stream session (like the quit combo).
    if (m_FirstPoll) {
        SDL_FlushEvent(SDL_CONTROLLERBUTTONDOWN);
        SDL_FlushEvent(SDL_CONTROLLERBUTTONUP);
        m_FirstPoll = false;
    }

    // Peep events rather than polling to avoid calling SDL_PumpEvents()
    while (SDL_PeepEvents(&event, 1, SDL_GETEVENT, SDL_FIRSTEVENT, SDL_LASTEVENT) == 1) {
        switch (event.type) {
        case SDL_QUIT:
            if (m_Sink != nullptr) {
                m_Sink->handleControllerQuit();
            }
            break;
        case SDL_CONTROLLERBUTTONDOWN:
        case SDL_CONTROLLERBUTTONUP:
        {
            bool pressed = event.type == SDL_CONTROLLERBUTTONDOWN;

            // Swap face buttons if needed
            if (m_Prefs->swapFaceButtons) {
                switch (event.cbutton.button) {
                case SDL_CONTROLLER_BUTTON_A:
                    event.cbutton.button = SDL_CONTROLLER_BUTTON_B;
                    break;
                case SDL_CONTROLLER_BUTTON_B:
                    event.cbutton.button = SDL_CONTROLLER_BUTTON_A;
                    break;
                case SDL_CONTROLLER_BUTTON_X:
                    event.cbutton.button = SDL_CONTROLLER_BUTTON_Y;
                    break;
                case SDL_CONTROLLER_BUTTON_Y:
                    event.cbutton.button = SDL_CONTROLLER_BUTTON_X;
                    break;
                }
            }

            switch (event.cbutton.button) {
            case SDL_CONTROLLER_BUTTON_DPAD_UP:
                sendAction(m_UiNavMode ? ControllerNavigationAction::PreviousControl : ControllerNavigationAction::Up,
                           pressed);
                break;
            case SDL_CONTROLLER_BUTTON_DPAD_DOWN:
                sendAction(m_UiNavMode ? ControllerNavigationAction::NextControl : ControllerNavigationAction::Down,
                           pressed);
                break;
            case SDL_CONTROLLER_BUTTON_DPAD_LEFT:
                sendAction(ControllerNavigationAction::Left, pressed);
                break;
            case SDL_CONTROLLER_BUTTON_DPAD_RIGHT:
                sendAction(ControllerNavigationAction::Right, pressed);
                break;
            case SDL_CONTROLLER_BUTTON_A:
                sendAction(m_UiNavMode ? ControllerNavigationAction::ActivateControl : ControllerNavigationAction::Accept,
                           pressed);
                break;
            case SDL_CONTROLLER_BUTTON_B:
                sendAction(ControllerNavigationAction::Back, pressed);
                break;
            case SDL_CONTROLLER_BUTTON_X:
                sendAction(ControllerNavigationAction::ContextMenu, pressed);
                break;
            case SDL_CONTROLLER_BUTTON_Y:
            case SDL_CONTROLLER_BUTTON_START:
                sendAction(ControllerNavigationAction::Settings, pressed);
                break;
            default:
                break;
            }
            break;
        }
        case SDL_CONTROLLERDEVICEADDED:
        {
            SDL_GameController* gc = SDL_GameControllerOpen(event.cdevice.which);
            if (gc != nullptr) {
                // SDL_CONTROLLERDEVICEADDED can be reported multiple times for the same
                // gamepad in rare cases, because SDL doesn't fixup the device index in
                // the SDL_CONTROLLERDEVICEADDED event if an unopened gamepad disappears
                // before we've processed the add event.
                if (!m_Gamepads.contains(gc)) {
                    m_Gamepads.append(gc);
                }
                else {
                    // We already have this game controller open
                    SDL_GameControllerClose(gc);
                }
            }
            break;
        }
        }
    }

    // Handle analog sticks by polling
    for (auto gc : std::as_const(m_Gamepads)) {
        short leftX = SDL_GameControllerGetAxis(gc, SDL_CONTROLLER_AXIS_LEFTX);
        short leftY = SDL_GameControllerGetAxis(gc, SDL_CONTROLLER_AXIS_LEFTY);
        if (SDL_GetTicks() - m_LastAxisNavigationEventTime < AXIS_NAVIGATION_REPEAT_DELAY) {
            // Do nothing
        }
        else if (leftY < -30000) {
            sendActionPress(m_UiNavMode ? ControllerNavigationAction::PreviousControl : ControllerNavigationAction::Up);
            m_LastAxisNavigationEventTime = SDL_GetTicks();
        }
        else if (leftY > 30000) {
            sendActionPress(m_UiNavMode ? ControllerNavigationAction::NextControl : ControllerNavigationAction::Down);
            m_LastAxisNavigationEventTime = SDL_GetTicks();
        }
        else if (leftX < -30000) {
            sendActionPress(ControllerNavigationAction::Left);
            m_LastAxisNavigationEventTime = SDL_GetTicks();
        }
        else if (leftX > 30000) {
            sendActionPress(ControllerNavigationAction::Right);
            m_LastAxisNavigationEventTime = SDL_GetTicks();
        }
    }
}

void SdlControllerNavigation::updateTimerState()
{
    if (m_PollingTimer->isActive() && (!m_HasFocus || !m_Enabled)) {
        m_PollingTimer->stop();
    }
    else if (!m_PollingTimer->isActive() && m_HasFocus && m_Enabled) {
        // Flush events on the first poll
        m_FirstPoll = true;

        // Poll every 50 ms for a new joystick event
        m_PollingTimer->start(50);
    }
}

void SdlControllerNavigation::setUiNavMode(bool uiNavMode)
{
    m_UiNavMode = uiNavMode;
}

int SdlControllerNavigation::getConnectedGamepads()
{
    Q_ASSERT(m_Enabled);

    int count = 0;
    int numJoysticks = SDL_NumJoysticks();
    for (int i = 0; i < numJoysticks; i++) {
        if (SDL_IsGameController(i)) {
            count++;
        }
    }

    return count;
}
