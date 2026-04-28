#pragma once

#include <QUrl>
#include <QString>

struct FrontendComputer
{
    QString name;
    QString details;
    bool online = false;
    bool paired = false;
    bool busy = false;
    bool wakeable = false;
    bool statusUnknown = false;
    bool serverSupported = false;
};

struct FrontendApp
{
    QString name;
    QUrl boxArt;
    int appId = 0;
    bool running = false;
    bool hidden = false;
    bool directLaunch = false;
    bool appCollectorGame = false;
};
