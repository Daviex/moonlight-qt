#pragma once

#include <QEvent>

#include "frontend/sdlcontrollernavigation.h"
#include "settings/streamingpreferences.h"

class SdlGamepadKeyNavigation : public QObject, public IControllerNavigationSink
{
    Q_OBJECT

public:
    SdlGamepadKeyNavigation(StreamingPreferences* prefs);

    ~SdlGamepadKeyNavigation();

    Q_INVOKABLE void enable();

    Q_INVOKABLE void disable();

    Q_INVOKABLE void notifyWindowFocus(bool hasFocus);

    Q_INVOKABLE void setUiNavMode(bool settingsMode);

    Q_INVOKABLE int getConnectedGamepads();

private:
    void handleControllerNavigation(ControllerNavigationAction action, bool pressed) override;
    void handleControllerQuit() override;

    void sendKey(QEvent::Type type, Qt::Key key, Qt::KeyboardModifiers modifiers = Qt::NoModifier);

private:
    SdlControllerNavigation* m_ControllerNavigation;
};
