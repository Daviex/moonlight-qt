#pragma once

#include "backend/boxartmanager.h"
#include "backend/computermanager.h"
#include "frontend/frontendtypes.h"
#include "streaming/session.h"

#include <QObject>
#include <QVariant>
#include <QVector>

class AppListFacade : public QObject
{
    Q_OBJECT

public:
    explicit AppListFacade(QObject* parent = nullptr);

    void initialize(ComputerManager* computerManager, int computerIndex, bool showHiddenGames);

    int count() const;
    QVector<FrontendApp> apps();
    FrontendApp appAt(int appIndex);

    Session* createSessionForApp(int appIndex);
    int getDirectLaunchAppIndex() const;
    int getRunningAppId() const;
    QString getRunningAppName() const;
    void quitRunningApp();
    void setAppHidden(int appIndex, bool hidden);
    void setAppDirectLaunch(int appIndex, bool directLaunch);

signals:
    void appsReset();
    void appChanged(int appIndex);
    void appBoxArtChanged(int appIndex, QUrl image);
    void quitAppCompleted(QString error);
    void computerLost();

private slots:
    void handleComputerStateChanged(NvComputer* computer);
    void handleBoxArtLoaded(QString computerUuid, NvApp app, QUrl image);
    void handleQuitAppCompleted(QVariant error);

private:
    bool isValidAppIndex(int appIndex, const char* operation) const;
    FrontendApp snapshotApp(NvApp app);
    void updateAppList(QVector<NvApp> newList);
    QVector<NvApp> getVisibleApps(const QVector<NvApp>& appList);
    bool isAppCurrentlyVisible(const NvApp& app) const;

    NvComputer* m_Computer = nullptr;
    QString m_ComputerUuid;
    BoxArtManager m_BoxArtManager;
    ComputerManager* m_ComputerManager = nullptr;
    QVector<NvApp> m_VisibleApps;
    QVector<NvApp> m_AllApps;
    int m_CurrentGameId = 0;
    bool m_ShowHiddenGames = false;
};
