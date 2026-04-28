#pragma once

enum class ControllerNavigationAction
{
    Up,
    Down,
    Left,
    Right,
    Accept,
    Back,
    ContextMenu,
    Settings,
    NextControl,
    PreviousControl,
    ActivateControl,
};

class IControllerNavigationSink
{
public:
    virtual ~IControllerNavigationSink() = default;

    virtual void handleControllerNavigation(ControllerNavigationAction action, bool pressed) = 0;
    virtual void handleControllerQuit() = 0;
};
