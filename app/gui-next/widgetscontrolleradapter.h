#pragma once

#include "frontend/controllernavigation.h"
#include "frontend/sdlcontrollernavigation.h"

#include <QObject>

class StreamingPreferences;

class WidgetsControllerAdapter : public QObject, public IControllerNavigationSink
{
    Q_OBJECT

public:
    explicit WidgetsControllerAdapter(StreamingPreferences* preferences, QObject* parent = nullptr);
    ~WidgetsControllerAdapter() override;

    void enable();
    void disable();
    void notifyWindowFocus(bool hasFocus);
    void setUiNavMode(bool uiNavMode);
    int connectedGamepads();

    void handleControllerNavigation(ControllerNavigationAction action, bool pressed) override;
    void handleControllerQuit() override;

private:
    void sendKey(int key, Qt::KeyboardModifiers modifiers, bool pressed);

    SdlControllerNavigation m_ControllerNavigation;
};
