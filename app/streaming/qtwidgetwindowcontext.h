#pragma once

#include "sessionwindowcontext.h"

#include <QPointer>

class QWidget;

class QtWidgetWindowContext : public SessionWindowContext
{
public:
    explicit QtWidgetWindowContext(QWidget* window);

    QRect screenGeometry() const override;
    SessionWindowState windowState() const override;
    void setMinimized(bool minimized) override;

private:
    QPointer<QWidget> m_Window;
};
