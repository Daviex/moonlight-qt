#pragma once

#include "backend/computermanager.h"
#include "frontend/applicationfacade.h"

#include <QJsonObject>
#include <QJsonValue>
#include <QCoreApplication>

class AppListFacade;

class TauriBridgeHelper
{
    Q_DECLARE_TR_FUNCTIONS(TauriBridgeHelper)

public:
    TauriBridgeHelper();

    int run();

private:
    QJsonObject handleCommand(const QJsonObject& command);
    QJsonObject listHosts();
    QJsonObject hostDetails(const QJsonObject& payload);
    QJsonObject listApps(const QJsonObject& payload);
    QJsonObject pairHost(const QJsonObject& payload);
    QJsonObject wakeHost(const QJsonObject& payload);
    QJsonObject renameHost(const QJsonObject& payload);
    QJsonObject deleteHost(const QJsonObject& payload);
    QJsonObject quitRunningApp(const QJsonObject& payload);
    QJsonObject setAppHidden(const QJsonObject& payload);
    QJsonObject setAppDirectLaunch(const QJsonObject& payload);
    QJsonObject loadSettings();
    QJsonObject saveSettings(const QJsonObject& payload);

    QJsonObject status(const QString& message) const;
    QJsonObject resultWithEvent(const QJsonValue& result, const QJsonObject& event) const;
    QJsonObject bridgeEvent(const QString& kind, const QString& message, const QString& hostId = QString(), const QString& appId = QString()) const;
    QJsonObject hostToJson(const FrontendComputer& computer, int index) const;
    QJsonObject appToJson(const FrontendApp& app) const;
    QString hostStatus(const FrontendComputer& computer) const;
    QString hostAddress(const FrontendComputer& computer) const;
    int hostIndexFromPayload(const QJsonObject& payload);
    int appIndexFromPayload(AppListFacade* appList, const QJsonObject& payload) const;
    QJsonObject unsupported(const QString& command) const;

    ComputerManager m_ComputerManager;
    FrontendApplicationFacade m_Facade;
};
