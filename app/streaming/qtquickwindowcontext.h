#pragma once

#include "sessionwindowcontext.h"

#include <QPointer>
#include <QQuickWindow>

class QtQuickWindowContext : public SessionWindowContext
{
public:
    explicit QtQuickWindowContext(QQuickWindow* window);

    QRect screenGeometry() const override;
    SessionWindowState windowState() const override;
    void setMinimized(bool minimized) override;

private:
    QPointer<QQuickWindow> m_Window;
};
