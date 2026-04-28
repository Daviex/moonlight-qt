#include "sdlgamepadkeynavigation.h"

#include <QCoreApplication>
#include <QKeyEvent>
#include <QGuiApplication>
#include <QWindow>

SdlGamepadKeyNavigation::SdlGamepadKeyNavigation(StreamingPreferences* prefs)
    : m_ControllerNavigation(new SdlControllerNavigation(prefs, this))
{
    m_ControllerNavigation->setSink(this);
}

SdlGamepadKeyNavigation::~SdlGamepadKeyNavigation()
{
    m_ControllerNavigation->disable();
    m_ControllerNavigation->setSink(nullptr);
}

void SdlGamepadKeyNavigation::enable()
{
    m_ControllerNavigation->enable();
}

void SdlGamepadKeyNavigation::disable()
{
    m_ControllerNavigation->disable();
}

void SdlGamepadKeyNavigation::notifyWindowFocus(bool hasFocus)
{
    m_ControllerNavigation->notifyWindowFocus(hasFocus);
}

void SdlGamepadKeyNavigation::handleControllerNavigation(ControllerNavigationAction action, bool pressed)
{
    QEvent::Type type = pressed ? QEvent::Type::KeyPress : QEvent::Type::KeyRelease;

    switch (action) {
    case ControllerNavigationAction::Up:
        sendKey(type, Qt::Key_Up);
        break;
    case ControllerNavigationAction::Down:
        sendKey(type, Qt::Key_Down);
        break;
    case ControllerNavigationAction::Left:
        sendKey(type, Qt::Key_Left);
        break;
    case ControllerNavigationAction::Right:
        sendKey(type, Qt::Key_Right);
        break;
    case ControllerNavigationAction::Accept:
        sendKey(type, Qt::Key_Return);
        break;
    case ControllerNavigationAction::Back:
        sendKey(type, Qt::Key_Escape);
        break;
    case ControllerNavigationAction::ContextMenu:
        sendKey(type, Qt::Key_Menu);
        break;
    case ControllerNavigationAction::Settings:
        sendKey(type, Qt::Key_Hangup);
        break;
    case ControllerNavigationAction::NextControl:
        sendKey(type, Qt::Key_Tab);
        break;
    case ControllerNavigationAction::PreviousControl:
        sendKey(type, Qt::Key_Tab, Qt::ShiftModifier);
        break;
    case ControllerNavigationAction::ActivateControl:
        sendKey(type, Qt::Key_Space);
        break;
    }
}

void SdlGamepadKeyNavigation::handleControllerQuit()
{
    // SDL may send us a quit event since we initialize the video subsystem on
    // startup. If we get one, forward it on for Qt to take care of.
    QCoreApplication::instance()->quit();
}

void SdlGamepadKeyNavigation::sendKey(QEvent::Type type, Qt::Key key, Qt::KeyboardModifiers modifiers)
{
    QGuiApplication* app = static_cast<QGuiApplication*>(QGuiApplication::instance());
    QWindow* focusWindow = app->focusWindow();
    if (focusWindow != nullptr) {
        QKeyEvent keyPressEvent(type, key, modifiers);
        app->sendEvent(focusWindow, &keyPressEvent);
    }
}

void SdlGamepadKeyNavigation::setUiNavMode(bool uiNavMode)
{
    m_ControllerNavigation->setUiNavMode(uiNavMode);
}

int SdlGamepadKeyNavigation::getConnectedGamepads()
{
    return m_ControllerNavigation->getConnectedGamepads();
}
