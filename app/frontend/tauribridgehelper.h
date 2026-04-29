#pragma once

#include "backend/computermanager.h"
#include "frontend/applicationfacade.h"
#include "frontend/applistfacade.h"
#include "frontend/controllernavigation.h"

#include <QJsonObject>
#include <QJsonValue>
#include <QCoreApplication>
#include <QPointer>
#include <QScopedPointer>

class QtWidgetWindowContext;
class SdlControllerNavigation;
class Session;
class QWidget;

class TauriBridgeHelper : public IControllerNavigationSink
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
    QJsonObject launchApp(const QJsonObject& payload);
    QJsonObject resumeSession(const QJsonObject& payload);
    QJsonObject pairHost(const QJsonObject& payload);
    QJsonObject wakeHost(const QJsonObject& payload);
    QJsonObject renameHost(const QJsonObject& payload);
    QJsonObject deleteHost(const QJsonObject& payload);
    QJsonObject testNetwork(const QJsonObject& payload);
    QJsonObject quitRunningApp(const QJsonObject& payload);
    QJsonObject setAppHidden(const QJsonObject& payload);
    QJsonObject setAppDirectLaunch(const QJsonObject& payload);
    QJsonObject loadSettings();
    QJsonObject saveSettings(const QJsonObject& payload);
    QJsonObject defaultBitrate(const QJsonObject& payload);
    QJsonObject systemInfo();
    QJsonObject openUrl(const QJsonObject& payload);

    void handleControllerNavigation(ControllerNavigationAction action, bool pressed) override;
    void handleControllerQuit() override;

    QJsonObject status(const QString& message) const;
    QJsonObject resultWithEvent(const QJsonValue& result, const QJsonObject& event) const;
    QJsonObject bridgeEvent(const QString& kind, const QString& message, const QString& hostId = QString(), const QString& appId = QString()) const;
    void writeEventFrame(const QJsonObject& event) const;
    void setControllerNavigationEnabled(bool enabled);
    void observeAppList(int hostIndex, bool showHiddenGames);
    QJsonObject startSession(Session* session, const QString& appName, bool isResume, const QString& hostId, const QString& appId);
    QString controllerActionName(ControllerNavigationAction action) const;
    QJsonObject hostToJson(const FrontendComputer& computer, int index) const;
    QJsonObject appToJson(const FrontendApp& app) const;
    QString hostStatus(const FrontendComputer& computer) const;
    QString hostAddress(const FrontendComputer& computer) const;
    int hostIndexFromPayload(const QJsonObject& payload, QString* error = nullptr);
    int appIndexFromPayload(AppListFacade* appList, const QJsonObject& payload, QString* error = nullptr) const;
    QJsonObject unsupported(const QString& command) const;

    ComputerManager m_ComputerManager;
    FrontendApplicationFacade m_Facade;
    QScopedPointer<SdlControllerNavigation> m_ControllerNavigation;
    QScopedPointer<AppListFacade> m_ObservedAppList;
    QString m_ObservedAppHostId;
    QScopedPointer<QWidget> m_WindowContextSource;
    QScopedPointer<QtWidgetWindowContext> m_WindowContext;
    QPointer<Session> m_ActiveSession;
    bool m_SuppressFacadeEvents = false;
};
