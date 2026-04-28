#include "applistfacade.h"

#include <QDebug>

#include <utility>

AppListFacade::AppListFacade(QObject* parent)
    : QObject(parent)
{
    connect(&m_BoxArtManager, &BoxArtManager::boxArtLoadComplete,
            this, &AppListFacade::handleBoxArtLoaded);
}

void AppListFacade::initialize(ComputerManager* computerManager, int computerIndex, bool showHiddenGames)
{
    m_ComputerManager = computerManager;
    connect(m_ComputerManager, &ComputerManager::computerStateChanged,
            this, &AppListFacade::handleComputerStateChanged);

    QVector<NvComputer*> computers = m_ComputerManager->getComputers();
    if (computerIndex < 0 || computerIndex >= computers.count()) {
        qWarning() << "AppListFacade::initialize called with invalid computer index:" << computerIndex;
        return;
    }

    m_Computer = computers.at(computerIndex);
    m_ShowHiddenGames = showHiddenGames;

    QVector<NvApp> appList;
    {
        QReadLocker lock(&m_Computer->lock);
        m_ComputerUuid = m_Computer->uuid;
        m_CurrentGameId = m_Computer->currentGameId;
        appList = m_Computer->appList;
    }

    updateAppList(appList);
}

bool AppListFacade::isValidAppIndex(int appIndex, const char* operation) const
{
    if (appIndex >= 0 && appIndex < m_VisibleApps.count()) {
        return true;
    }

    qWarning() << operation << "called with invalid app index:" << appIndex;
    return false;
}

int AppListFacade::count() const
{
    return m_VisibleApps.count();
}

QVector<FrontendApp> AppListFacade::apps()
{
    QVector<FrontendApp> snapshot;
    snapshot.reserve(m_VisibleApps.count());

    for (const NvApp& app : std::as_const(m_VisibleApps)) {
        snapshot.append(snapshotApp(app));
    }

    return snapshot;
}

FrontendApp AppListFacade::appAt(int appIndex)
{
    if (!isValidAppIndex(appIndex, "AppListFacade::appAt")) {
        return {};
    }

    return snapshotApp(m_VisibleApps.at(appIndex));
}

FrontendApp AppListFacade::snapshotApp(NvApp app)
{
    FrontendApp snapshot;
    snapshot.name = app.name;
    snapshot.running = m_CurrentGameId == app.id;
    snapshot.boxArt = m_BoxArtManager.loadBoxArt(m_Computer, app);
    snapshot.hidden = app.hidden;
    snapshot.appId = app.id;
    snapshot.directLaunch = app.directLaunch;
    snapshot.appCollectorGame = app.isAppCollectorGame;
    return snapshot;
}

Session* AppListFacade::createSessionForApp(int appIndex)
{
    if (!isValidAppIndex(appIndex, "AppListFacade::createSessionForApp")) {
        return nullptr;
    }

    NvApp app = m_VisibleApps.at(appIndex);
    return new Session(m_Computer, app);
}

int AppListFacade::getDirectLaunchAppIndex() const
{
    for (int i = 0; i < m_VisibleApps.count(); i++) {
        if (m_VisibleApps[i].directLaunch) {
            return i;
        }
    }

    return -1;
}

int AppListFacade::getRunningAppId() const
{
    return m_CurrentGameId;
}

QString AppListFacade::getRunningAppName() const
{
    if (m_CurrentGameId != 0) {
        for (int i = 0; i < m_AllApps.count(); i++) {
            if (m_AllApps[i].id == m_CurrentGameId) {
                return m_AllApps[i].name;
            }
        }
    }

    return QString();
}

void AppListFacade::quitRunningApp()
{
    m_ComputerManager->quitRunningApp(m_Computer);
}

bool AppListFacade::isAppCurrentlyVisible(const NvApp& app) const
{
    for (const NvApp& visibleApp : m_VisibleApps) {
        if (app.id == visibleApp.id) {
            return true;
        }
    }

    return false;
}

QVector<NvApp> AppListFacade::getVisibleApps(const QVector<NvApp>& appList)
{
    QVector<NvApp> visibleApps;

    for (const NvApp& app : appList) {
        if (m_ShowHiddenGames || !app.hidden || isAppCurrentlyVisible(app)) {
            visibleApps.append(app);
        }
    }

    return visibleApps;
}

void AppListFacade::updateAppList(QVector<NvApp> newList)
{
    m_AllApps = newList;
    m_VisibleApps = getVisibleApps(newList);
    emit appsReset();
}

void AppListFacade::setAppHidden(int appIndex, bool hidden)
{
    if (!isValidAppIndex(appIndex, "AppListFacade::setAppHidden")) {
        return;
    }

    int appId = m_VisibleApps.at(appIndex).id;

    {
        QWriteLocker lock(&m_Computer->lock);

        for (NvApp& app : m_Computer->appList) {
            if (app.id == appId) {
                app.hidden = hidden;
                break;
            }
        }
    }

    m_ComputerManager->clientSideAttributeUpdated(m_Computer);
}

void AppListFacade::setAppDirectLaunch(int appIndex, bool directLaunch)
{
    if (!isValidAppIndex(appIndex, "AppListFacade::setAppDirectLaunch")) {
        return;
    }

    int appId = m_VisibleApps.at(appIndex).id;

    {
        QWriteLocker lock(&m_Computer->lock);

        for (NvApp& app : m_Computer->appList) {
            if (directLaunch) {
                app.directLaunch = app.id == appId;
            }
            else if (app.id == appId) {
                app.directLaunch = false;
                break;
            }
        }
    }

    m_ComputerManager->clientSideAttributeUpdated(m_Computer);
}

void AppListFacade::handleComputerStateChanged(NvComputer* computer)
{
    if (computer != m_Computer) {
        return;
    }

    NvComputer::ComputerState state;
    NvComputer::PairState pairState;
    QVector<NvApp> appList;
    int currentGameId;
    {
        QReadLocker lock(&computer->lock);
        state = computer->state;
        pairState = computer->pairState;
        appList = computer->appList;
        currentGameId = computer->currentGameId;
    }

    if (state == NvComputer::CS_OFFLINE ||
        pairState == NvComputer::PS_NOT_PAIRED) {
        emit computerLost();
        return;
    }

    if (appList != m_AllApps) {
        updateAppList(appList);
    }

    if (currentGameId != m_CurrentGameId) {
        m_CurrentGameId = currentGameId;
        emit appsReset();
    }
}

void AppListFacade::handleBoxArtLoaded(QString computerUuid, NvApp app, QUrl image)
{
    if (computerUuid != m_ComputerUuid) {
        return;
    }

    for (int i = 0; i < m_VisibleApps.count(); i++) {
        if (m_VisibleApps[i].id == app.id) {
            emit appBoxArtChanged(i, image);
            emit appChanged(i);
            return;
        }
    }
}
