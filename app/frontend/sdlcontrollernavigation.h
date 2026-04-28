#pragma once

#include <QList>
#include <QObject>
#include <QTimer>

#include "SDL_compat.h"
#include "frontend/controllernavigation.h"
#include "settings/streamingpreferences.h"

class SdlControllerNavigation : public QObject
{
public:
    explicit SdlControllerNavigation(StreamingPreferences* prefs, QObject* parent = nullptr);
    ~SdlControllerNavigation() override;

    void setSink(IControllerNavigationSink* sink);

    void enable();
    void disable();
    void notifyWindowFocus(bool hasFocus);
    void setUiNavMode(bool uiNavMode);
    int getConnectedGamepads();

private:
    void sendAction(ControllerNavigationAction action, bool pressed);
    void sendActionPress(ControllerNavigationAction action);
    void updateTimerState();
    void onPollingTimerFired();

    StreamingPreferences* m_Prefs;
    IControllerNavigationSink* m_Sink;
    QTimer* m_PollingTimer;
    QList<SDL_GameController*> m_Gamepads;
    bool m_Enabled;
    bool m_UiNavMode;
    bool m_FirstPoll;
    bool m_HasFocus;
    Uint32 m_LastAxisNavigationEventTime;
};
