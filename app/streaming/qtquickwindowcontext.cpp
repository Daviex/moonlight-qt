#include "qtquickwindowcontext.h"

#include <QScreen>

QtQuickWindowContext::QtQuickWindowContext(QQuickWindow* window)
    : m_Window(window)
{
}

QRect QtQuickWindowContext::screenGeometry() const
{
    if (m_Window == nullptr || m_Window->screen() == nullptr) {
        return {};
    }

    return m_Window->screen()->geometry();
}

SessionWindowState QtQuickWindowContext::windowState() const
{
    SessionWindowState state;
    if (m_Window == nullptr) {
        return state;
    }

#if QT_VERSION >= QT_VERSION_CHECK(5, 10, 0)
    Qt::WindowStates states = m_Window->windowStates();
    state.maximized = states & Qt::WindowMaximized;
    state.minimized = states & Qt::WindowMinimized;
#else
    state.maximized = m_Window->windowState() == Qt::WindowMaximized;
    state.minimized = m_Window->windowState() == Qt::WindowMinimized;
#endif

    return state;
}

void QtQuickWindowContext::setMinimized(bool minimized)
{
    if (m_Window == nullptr) {
        return;
    }

#if QT_VERSION >= QT_VERSION_CHECK(5, 10, 0)
    if (minimized) {
        m_Window->setWindowStates(m_Window->windowStates() | Qt::WindowMinimized);
    }
    else if (m_Window->windowStates() & Qt::WindowMinimized) {
        m_Window->setWindowStates(m_Window->windowStates() & ~Qt::WindowMinimized);
    }
#else
    if (minimized) {
        m_Window->setWindowState(Qt::WindowMinimized);
    }
    else if (m_Window->windowState() & Qt::WindowMinimized) {
        m_Window->setWindowState(Qt::WindowNoState);
    }
#endif
}
