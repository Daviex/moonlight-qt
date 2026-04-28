#pragma once

#include <QRect>

struct SessionWindowState
{
    bool maximized = false;
    bool minimized = false;
};

class SessionWindowContext
{
public:
    virtual ~SessionWindowContext() = default;

    virtual QRect screenGeometry() const = 0;
    virtual SessionWindowState windowState() const = 0;
    virtual void setMinimized(bool minimized) = 0;
};
