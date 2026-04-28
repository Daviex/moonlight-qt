#include "qtwidgetwindowcontext.h"

#include <QGuiApplication>
#include <QScreen>
#include <QWidget>
#include <QWindow>

QtWidgetWindowContext::QtWidgetWindowContext(QWidget* window)
    : m_Window(window)
{
}

QRect QtWidgetWindowContext::screenGeometry() const
{
    QScreen* screen = nullptr;
    if (m_Window != nullptr && m_Window->windowHandle() != nullptr) {
        screen = m_Window->windowHandle()->screen();
    }
    if (screen == nullptr) {
        screen = QGuiApplication::primaryScreen();
    }

    return screen != nullptr ? screen->geometry() : QRect();
}

SessionWindowState QtWidgetWindowContext::windowState() const
{
    SessionWindowState state;
    if (m_Window == nullptr) {
        return state;
    }

    Qt::WindowStates states = m_Window->windowState();
    state.maximized = states & Qt::WindowMaximized;
    state.minimized = states & Qt::WindowMinimized;
    return state;
}

void QtWidgetWindowContext::setMinimized(bool minimized)
{
    if (m_Window == nullptr) {
        return;
    }

    Qt::WindowStates states = m_Window->windowState();
    if (minimized) {
        m_Window->setWindowState(states | Qt::WindowMinimized);
    }
    else if (states & Qt::WindowMinimized) {
        m_Window->setWindowState(states & ~Qt::WindowMinimized);
    }
}
