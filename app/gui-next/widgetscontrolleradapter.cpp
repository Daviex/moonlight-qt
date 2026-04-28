#include "widgetscontrolleradapter.h"

#include <QApplication>
#include <QKeyEvent>
#include <QWidget>

WidgetsControllerAdapter::WidgetsControllerAdapter(StreamingPreferences* preferences, QObject* parent)
    : QObject(parent),
      m_ControllerNavigation(preferences, this)
{
    m_ControllerNavigation.setSink(this);
}

WidgetsControllerAdapter::~WidgetsControllerAdapter()
{
    m_ControllerNavigation.disable();
    m_ControllerNavigation.setSink(nullptr);
}

void WidgetsControllerAdapter::enable()
{
    m_ControllerNavigation.enable();
}

void WidgetsControllerAdapter::disable()
{
    m_ControllerNavigation.disable();
}

void WidgetsControllerAdapter::notifyWindowFocus(bool hasFocus)
{
    m_ControllerNavigation.notifyWindowFocus(hasFocus);
}

void WidgetsControllerAdapter::setUiNavMode(bool uiNavMode)
{
    m_ControllerNavigation.setUiNavMode(uiNavMode);
}

int WidgetsControllerAdapter::connectedGamepads()
{
    return m_ControllerNavigation.getConnectedGamepads();
}

void WidgetsControllerAdapter::handleControllerNavigation(ControllerNavigationAction action, bool pressed)
{
    switch (action) {
    case ControllerNavigationAction::Up:
        sendKey(Qt::Key_Up, Qt::NoModifier, pressed);
        break;
    case ControllerNavigationAction::Down:
        sendKey(Qt::Key_Down, Qt::NoModifier, pressed);
        break;
    case ControllerNavigationAction::Left:
        sendKey(Qt::Key_Left, Qt::NoModifier, pressed);
        break;
    case ControllerNavigationAction::Right:
        sendKey(Qt::Key_Right, Qt::NoModifier, pressed);
        break;
    case ControllerNavigationAction::Accept:
        sendKey(Qt::Key_Return, Qt::NoModifier, pressed);
        break;
    case ControllerNavigationAction::Back:
        sendKey(Qt::Key_Escape, Qt::NoModifier, pressed);
        break;
    case ControllerNavigationAction::ContextMenu:
        sendKey(Qt::Key_Menu, Qt::NoModifier, pressed);
        break;
    case ControllerNavigationAction::Settings:
        sendKey(Qt::Key_Hangup, Qt::NoModifier, pressed);
        break;
    case ControllerNavigationAction::NextControl:
        sendKey(Qt::Key_Tab, Qt::NoModifier, pressed);
        break;
    case ControllerNavigationAction::PreviousControl:
        sendKey(Qt::Key_Tab, Qt::ShiftModifier, pressed);
        break;
    case ControllerNavigationAction::ActivateControl:
        sendKey(Qt::Key_Space, Qt::NoModifier, pressed);
        break;
    }
}

void WidgetsControllerAdapter::handleControllerQuit()
{
    qApp->quit();
}

void WidgetsControllerAdapter::sendKey(int key, Qt::KeyboardModifiers modifiers, bool pressed)
{
    QWidget* target = QApplication::focusWidget();
    if (target == nullptr) {
        target = QApplication::activeWindow();
    }
    if (target == nullptr) {
        return;
    }

    QEvent::Type type = pressed ? QEvent::KeyPress : QEvent::KeyRelease;
    auto event = new QKeyEvent(type, key, modifiers);
    QApplication::postEvent(target, event);
}
