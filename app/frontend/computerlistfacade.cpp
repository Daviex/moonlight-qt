#include "computerlistfacade.h"

#include <Limelight.h>

#include <QDebug>
#include <QThreadPool>

ComputerListFacade::ComputerListFacade(QObject* parent)
    : QObject(parent)
{
}

void ComputerListFacade::initialize(ComputerManager* computerManager)
{
    m_ComputerManager = computerManager;
    connect(m_ComputerManager, &ComputerManager::computerStateChanged,
            this, &ComputerListFacade::handleComputerStateChanged);
    connect(m_ComputerManager, &ComputerManager::pairingCompleted,
            this, &ComputerListFacade::handlePairingCompleted);

    m_Computers = m_ComputerManager->getComputers();
}

bool ComputerListFacade::isValidComputerIndex(int computerIndex, const char* operation) const
{
    if (computerIndex >= 0 && computerIndex < m_Computers.count()) {
        return true;
    }

    qWarning() << operation << "called with invalid computer index:" << computerIndex;
    return false;
}

int ComputerListFacade::count() const
{
    return m_Computers.count();
}

QVector<FrontendComputer> ComputerListFacade::computers() const
{
    QVector<FrontendComputer> snapshot;
    snapshot.reserve(m_Computers.count());

    for (NvComputer* computer : m_Computers) {
        snapshot.append(snapshotComputer(computer));
    }

    return snapshot;
}

FrontendComputer ComputerListFacade::computerAt(int computerIndex) const
{
    if (!isValidComputerIndex(computerIndex, "ComputerListFacade::computerAt")) {
        return {};
    }

    return snapshotComputer(m_Computers[computerIndex]);
}

FrontendComputer ComputerListFacade::snapshotComputer(NvComputer* computer) const
{
    FrontendComputer snapshot;
    QReadLocker lock(&computer->lock);

    snapshot.name = computer->name;
    snapshot.online = computer->state == NvComputer::CS_ONLINE;
    snapshot.paired = computer->pairState == NvComputer::PS_PAIRED;
    snapshot.busy = computer->currentGameId != 0;
    snapshot.wakeable = !computer->macAddress.isEmpty();
    snapshot.statusUnknown = computer->state == NvComputer::CS_UNKNOWN;
    snapshot.serverSupported = computer->isSupportedServerVersion;
    snapshot.activeAddress = computer->activeAddress.toString();
    snapshot.uuid = computer->uuid;
    snapshot.localAddress = computer->localAddress.toString();
    snapshot.remoteAddress = computer->remoteAddress.toString();
    snapshot.ipv6Address = computer->ipv6Address.toString();
    snapshot.manualAddress = computer->manualAddress.toString();
    snapshot.macAddress = computer->macAddress.isEmpty() ? tr("Unknown") : QString(computer->macAddress.toHex(':'));
    snapshot.runningGameId = computer->currentGameId;
    snapshot.httpsPort = computer->activeHttpsPort;
    snapshot.appVersion = computer->appVersion;
    snapshot.gfeVersion = computer->gfeVersion;
    snapshot.gpuModel = computer->gpuModel;

    QString state;
    switch (computer->state) {
    case NvComputer::CS_ONLINE:
        state = tr("Online");
        break;
    case NvComputer::CS_OFFLINE:
        state = tr("Offline");
        break;
    default:
        state = tr("Unknown");
        break;
    }

    QString pairState;
    switch (computer->pairState) {
    case NvComputer::PS_PAIRED:
        pairState = tr("Paired");
        break;
    case NvComputer::PS_NOT_PAIRED:
        pairState = tr("Unpaired");
        break;
    default:
        pairState = tr("Unknown");
        break;
    }
    snapshot.pairState = pairState;

    snapshot.details = tr("Name: %1").arg(computer->name) + '\n' +
                       tr("Status: %1").arg(state) + '\n' +
                       tr("Active Address: %1").arg(computer->activeAddress.toString()) + '\n' +
                       tr("UUID: %1").arg(computer->uuid) + '\n' +
                       tr("Local Address: %1").arg(computer->localAddress.toString()) + '\n' +
                       tr("Remote Address: %1").arg(computer->remoteAddress.toString()) + '\n' +
                       tr("IPv6 Address: %1").arg(computer->ipv6Address.toString()) + '\n' +
                       tr("Manual Address: %1").arg(computer->manualAddress.toString()) + '\n' +
                       tr("MAC Address: %1").arg(computer->macAddress.isEmpty() ? tr("Unknown") : QString(computer->macAddress.toHex(':'))) + '\n' +
                       tr("Pair State: %1").arg(pairState) + '\n' +
                       tr("Running Game ID: %1").arg(computer->state == NvComputer::CS_ONLINE ? QString::number(computer->currentGameId) : tr("Unknown")) + '\n' +
                       tr("HTTPS Port: %1").arg(computer->state == NvComputer::CS_ONLINE ? QString::number(computer->activeHttpsPort) : tr("Unknown"));

    return snapshot;
}

Session* ComputerListFacade::createSessionForCurrentGame(int computerIndex)
{
    if (!isValidComputerIndex(computerIndex, "ComputerListFacade::createSessionForCurrentGame")) {
        return nullptr;
    }

    NvComputer* computer = m_Computers[computerIndex];

    int currentGameId;
    QVector<NvApp> appList;
    {
        QReadLocker lock(&computer->lock);
        currentGameId = computer->currentGameId;
        appList = computer->appList;
    }

    if (currentGameId == 0) {
        Q_ASSERT(currentGameId != 0);
        qWarning() << "Cannot resume stream without a running game";
        return nullptr;
    }

    for (NvApp& app : appList) {
        if (app.id == currentGameId) {
            return new Session(computer, app);
        }
    }

    Q_ASSERT(false);
    qWarning() << "Running game not found in app list:" << currentGameId;
    return nullptr;
}

void ComputerListFacade::deleteComputer(int computerIndex)
{
    if (!isValidComputerIndex(computerIndex, "ComputerListFacade::deleteComputer")) {
        return;
    }

    m_ComputerManager->deleteHost(m_Computers[computerIndex]);
    m_Computers.removeAt(computerIndex);
    emit computersReset();
}

class FrontendDeferredWakeHostTask : public QRunnable
{
public:
    FrontendDeferredWakeHostTask(NvComputer* computer)
        : m_Computer(computer)
    {
        setAutoDelete(true);
    }

    void run() override
    {
        m_Computer->wake();
    }

private:
    NvComputer* m_Computer;
};

void ComputerListFacade::wakeComputer(int computerIndex)
{
    if (!isValidComputerIndex(computerIndex, "ComputerListFacade::wakeComputer")) {
        return;
    }

    auto wakeTask = new FrontendDeferredWakeHostTask(m_Computers[computerIndex]);
    QThreadPool::globalInstance()->start(wakeTask);
}

void ComputerListFacade::renameComputer(int computerIndex, const QString& name)
{
    if (!isValidComputerIndex(computerIndex, "ComputerListFacade::renameComputer")) {
        return;
    }

    m_ComputerManager->renameHost(m_Computers[computerIndex], name);
}

QString ComputerListFacade::generatePinString()
{
    return m_ComputerManager->generatePinString();
}

class FrontendDeferredTestConnectionTask : public QObject, public QRunnable
{
    Q_OBJECT

public:
    FrontendDeferredTestConnectionTask()
    {
        setAutoDelete(true);
    }

    void run() override
    {
        unsigned int portTestResult = LiTestClientConnectivity("qt.conntest.moonlight-stream.org", 443, ML_PORT_FLAG_ALL);
        if (portTestResult == ML_TEST_RESULT_INCONCLUSIVE) {
            emit connectionTestCompleted(-1, QString());
        }
        else {
            char blockedPorts[512];
            LiStringifyPortFlags(portTestResult, "\n", blockedPorts, sizeof(blockedPorts));
            emit connectionTestCompleted(portTestResult, QString(blockedPorts));
        }
    }

signals:
    void connectionTestCompleted(int result, QString blockedPorts);
};

void ComputerListFacade::testConnectionForComputer(int computerIndex)
{
    if (!isValidComputerIndex(computerIndex, "ComputerListFacade::testConnectionForComputer")) {
        return;
    }

    auto testConnectionTask = new FrontendDeferredTestConnectionTask();
    QObject::connect(testConnectionTask, &FrontendDeferredTestConnectionTask::connectionTestCompleted,
                     this, &ComputerListFacade::connectionTestCompleted);
    QThreadPool::globalInstance()->start(testConnectionTask);
}

void ComputerListFacade::pairComputer(int computerIndex, const QString& pin)
{
    if (!isValidComputerIndex(computerIndex, "ComputerListFacade::pairComputer")) {
        return;
    }

    m_ComputerManager->pairHost(m_Computers[computerIndex], pin);
}

void ComputerListFacade::handlePairingCompleted(NvComputer*, QString error)
{
    emit pairingCompleted(error);
}

void ComputerListFacade::handleComputerStateChanged(NvComputer* computer)
{
    QVector<NvComputer*> newComputerList = m_ComputerManager->getComputers();

    if (m_Computers != newComputerList) {
        m_Computers = newComputerList;
        emit computersReset();
    }
    else {
        int index = m_Computers.indexOf(computer);
        if (index >= 0) {
            emit computerChanged(index);
        }
    }
}

#include "computerlistfacade.moc"
