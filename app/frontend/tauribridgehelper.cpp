#include "tauribridgehelper.h"

#include "frontend/applistfacade.h"
#include "frontend/sdlcontrollernavigation.h"
#include "settings/streamingpreferences.h"
#include "streaming/qtwidgetwindowcontext.h"

#include <QHash>
#include <QJsonArray>
#include <QJsonDocument>
#include <QSet>
#include <QStringList>
#include <QTextStream>
#include <QThread>
#include <QWidget>

#ifdef Q_OS_WIN
#include <qt_windows.h>

static bool readBridgeLine(HANDLE inputHandle, QByteArray& line)
{
    line.clear();

    char ch = 0;
    DWORD bytesRead = 0;
    while (true) {
        DWORD bytesAvailable = 0;
        if (!PeekNamedPipe(inputHandle, nullptr, 0, nullptr, &bytesAvailable, nullptr)) {
            return !line.isEmpty();
        }
        if (bytesAvailable == 0) {
            QCoreApplication::processEvents(QEventLoop::AllEvents, 10);
            QThread::msleep(10);
            continue;
        }
        if (!ReadFile(inputHandle, &ch, 1, &bytesRead, nullptr) || bytesRead == 0) {
            break;
        }
        if (ch == '\n') {
            break;
        }
        if (ch != '\r') {
            line.append(ch);
        }
    }

    return !line.isEmpty();
}

static bool writeBridgeLine(HANDLE outputHandle, const QByteArray& line)
{
    DWORD bytesWritten = 0;
    if (!WriteFile(outputHandle, line.constData(), static_cast<DWORD>(line.size()), &bytesWritten, nullptr)) {
        return false;
    }

    const char newline = '\n';
    return WriteFile(outputHandle, &newline, 1, &bytesWritten, nullptr);
}
#endif

TauriBridgeHelper::TauriBridgeHelper()
    : m_ComputerManager(StreamingPreferences::get())
{
    m_Facade.initialize(&m_ComputerManager);
    m_ControllerNavigation.reset(new SdlControllerNavigation(StreamingPreferences::get()));
    m_ControllerNavigation->setSink(this);
    m_ControllerNavigation->notifyWindowFocus(true);
    m_ControllerNavigation->enable();

    QObject::connect(m_Facade.computers(), &ComputerListFacade::computersReset,
                     [this]() {
        if (!m_SuppressFacadeEvents) {
            writeEventFrame(bridgeEvent("hostChanged", tr("Host list changed.")));
        }
    });
    QObject::connect(m_Facade.computers(), &ComputerListFacade::computerChanged,
                     [this](int computerIndex) {
        if (!m_SuppressFacadeEvents) {
            writeEventFrame(bridgeEvent("hostChanged", tr("Host changed."), QString::number(computerIndex)));
        }
    });
    QObject::connect(m_Facade.computers(), &ComputerListFacade::pairingCompleted,
                     [this](const QString& error) {
        if (!m_SuppressFacadeEvents) {
            if (error.isEmpty()) {
                writeEventFrame(bridgeEvent("hostChanged", tr("Pairing completed.")));
            }
            else {
                writeEventFrame(bridgeEvent("status", error));
            }
        }
    });
    QObject::connect(m_Facade.computers(), &ComputerListFacade::connectionTestCompleted,
                     [this](int result, const QString& blockedPorts) {
        if (!m_SuppressFacadeEvents) {
            const QString message = blockedPorts.isEmpty() ?
                tr("Network test completed with result %1.").arg(result) :
                tr("Network test completed with result %1. Blocked ports: %2").arg(result).arg(blockedPorts);
            writeEventFrame(bridgeEvent("status", message));
        }
    });

    QObject::connect(m_Facade.sessions(), &FrontendSessionCoordinator::stageTextChanged,
                     [this](const QString& stageText) {
        writeEventFrame(bridgeEvent("sessionChanged", stageText));
    });
    QObject::connect(m_Facade.sessions(), &FrontendSessionCoordinator::errorTextChanged,
                     [this](const QString& errorText) {
        if (!errorText.isEmpty()) {
            writeEventFrame(bridgeEvent("status", errorText));
        }
    });
    QObject::connect(m_Facade.sessions(), &FrontendSessionCoordinator::launchWarningsChanged,
                     [this](const QStringList& warnings) {
        for (const QString& warning : warnings) {
            writeEventFrame(bridgeEvent("status", warning));
        }
    });
    QObject::connect(m_Facade.sessions(), &FrontendSessionCoordinator::hideUiRequested,
                     [this]() {
        setControllerNavigationEnabled(false);
        writeEventFrame(bridgeEvent("sessionChanged", tr("Stream connected; hide UI requested.")));
    });
    QObject::connect(m_Facade.sessions(), &FrontendSessionCoordinator::showUiRequested,
                     [this]() {
        setControllerNavigationEnabled(true);
        writeEventFrame(bridgeEvent("sessionChanged", tr("Stream UI can be shown.")));
    });
    QObject::connect(m_Facade.sessions(), &FrontendSessionCoordinator::quitSegueRequested,
                     [this](const QString& appName) {
        writeEventFrame(bridgeEvent("sessionChanged", tr("Quitting %1...").arg(appName)));
    });
    QObject::connect(m_Facade.sessions(), &FrontendSessionCoordinator::sessionFinished,
                     [this](int portTestResult) {
        setControllerNavigationEnabled(true);
        writeEventFrame(bridgeEvent("sessionChanged", tr("Stream finished with port test result %1.").arg(portTestResult)));
    });
    QObject::connect(m_Facade.sessions(), &FrontendSessionCoordinator::sessionReadyForDeletion,
                     [this]() {
        setControllerNavigationEnabled(true);
        writeEventFrame(bridgeEvent("sessionChanged", tr("Stream session cleanup completed.")));
    });

    m_ComputerManager.startPolling();
}

int TauriBridgeHelper::run()
{
    auto processLine = [this](const QByteArray& rawLine) -> QVector<QByteArray> {
        const QString line = QString::fromUtf8(rawLine).trimmed();
        if (line.isEmpty()) {
            return {};
        }

        QJsonObject response;
        const QJsonDocument requestDocument = QJsonDocument::fromJson(line.toUtf8());
        const QJsonObject request = requestDocument.object();
        const int requestId = request.value("id").toInt();
        response.insert("id", requestId);

        QJsonArray events;
        if (!requestDocument.isObject() || !request.value("command").isObject()) {
            response.insert("error", "Invalid bridge request.");
        }
        else {
            const QJsonObject command = request.value("command").toObject();
            m_SuppressFacadeEvents = true;
            const QJsonObject result = handleCommand(command);
            m_SuppressFacadeEvents = false;
            if (result.contains("error")) {
                response.insert("error", result.value("error").toString());
            }
            else {
                response.insert("result", result.value("result"));
                events = result.value("events").toArray();
            }
        }

        QVector<QByteArray> lines;
        lines.append(QJsonDocument(response).toJson(QJsonDocument::Compact));
        for (const QJsonValue& event : events) {
            if (event.isObject()) {
                lines.append(QJsonDocument(QJsonObject{{"event", event.toObject()}}).toJson(QJsonDocument::Compact));
            }
        }
        return lines;
    };

#ifdef Q_OS_WIN
    HANDLE inputHandle = GetStdHandle(STD_INPUT_HANDLE);
    HANDLE outputHandle = GetStdHandle(STD_OUTPUT_HANDLE);
    if (inputHandle == INVALID_HANDLE_VALUE || inputHandle == nullptr ||
            outputHandle == INVALID_HANDLE_VALUE || outputHandle == nullptr) {
        return 1;
    }

    QByteArray line;
    while (readBridgeLine(inputHandle, line)) {
        const QVector<QByteArray> responses = processLine(line);
        for (const QByteArray& response : responses) {
            if (!response.isEmpty() && !writeBridgeLine(outputHandle, response)) {
                return 1;
            }
        }
    }
#else
    QTextStream input(stdin, QIODevice::ReadOnly);
    QTextStream output(stdout, QIODevice::WriteOnly);

    while (!input.atEnd()) {
        const QVector<QByteArray> responses = processLine(input.readLine().toUtf8());
        for (const QByteArray& response : responses) {
            if (!response.isEmpty()) {
                output << QString::fromUtf8(response) << Qt::endl;
                output.flush();
            }
        }
    }
#endif

    return 0;
}

QJsonObject TauriBridgeHelper::handleCommand(const QJsonObject& command)
{
    const QString commandName = command.value("command").toString();
    const QJsonObject payload = command.value("payload").toObject();

    if (commandName == "list_hosts") {
        return listHosts();
    }
    if (commandName == "add_host") {
        const QString address = payload.value("address").toString();
        if (address.isEmpty()) {
            return {{"error", "Host address is required."}};
        }
        m_ComputerManager.addNewHostManually(address);
        const QString message = tr("Host add requested.");
        return resultWithEvent(
            QJsonObject{{"status", status(message)}, {"hostId", address}},
            bridgeEvent("hostChanged", message, address));
    }
    if (commandName == "pair_host") {
        return pairHost(payload);
    }
    if (commandName == "wake_host") {
        return wakeHost(payload);
    }
    if (commandName == "rename_host") {
        return renameHost(payload);
    }
    if (commandName == "delete_host") {
        return deleteHost(payload);
    }
    if (commandName == "host_details") {
        return hostDetails(payload);
    }
    if (commandName == "test_network") {
        return testNetwork(payload);
    }
    if (commandName == "list_apps") {
        return listApps(payload);
    }
    if (commandName == "launch_app") {
        return launchApp(payload);
    }
    if (commandName == "resume_session") {
        return resumeSession(payload);
    }
    if (commandName == "quit_running_app") {
        return quitRunningApp(payload);
    }
    if (commandName == "set_app_hidden") {
        return setAppHidden(payload);
    }
    if (commandName == "set_app_direct_launch") {
        return setAppDirectLaunch(payload);
    }
    if (commandName == "load_settings") {
        return loadSettings();
    }
    if (commandName == "save_settings") {
        return saveSettings(payload);
    }

    return {{"error", QString("Unknown bridge command: %1").arg(commandName)}};
}

QJsonObject TauriBridgeHelper::listHosts()
{
    QJsonArray hosts;
    QSet<QString> seenHosts;
    QHash<QString, int> placeholderHostIndexes;
    auto hostQuality = [](const QJsonObject& host) {
        int quality = 0;
        if (!host.value("address").toString().isEmpty()) {
            quality += 4;
        }
        if (host.value("paired").toBool()) {
            quality += 2;
        }
        if (host.value("running").toBool()) {
            quality += 1;
        }
        if (host.value("status").toString() == "Online") {
            quality += 1;
        }
        return quality;
    };

    const QVector<FrontendComputer> computers = m_Facade.computers()->computers();
    for (int i = 0; i < computers.count(); i++) {
        const QJsonObject host = hostToJson(computers[i], i);
        const QString hostKey = QStringList{
            host.value("name").toString(),
            host.value("address").toString(),
            host.value("status").toString(),
            host.value("paired").toBool() ? "1" : "0",
            host.value("running").toBool() ? "1" : "0",
        }.join('\x1f');
        if (seenHosts.contains(hostKey)) {
            continue;
        }

        seenHosts.insert(hostKey);
        const QString nameKey = host.value("name").toString().toCaseFolded();
        const int existingHostIndex = placeholderHostIndexes.value(nameKey, -1);
        if (existingHostIndex >= 0) {
            const QJsonObject existingHost = hosts[existingHostIndex].toObject();
            const bool samePlaceholderHost =
                existingHost.value("address").toString().isEmpty() ||
                host.value("address").toString().isEmpty();
            if (samePlaceholderHost) {
                if (hostQuality(host) > hostQuality(existingHost)) {
                    hosts.replace(existingHostIndex, host);
                }
                continue;
            }
        }

        placeholderHostIndexes.insert(nameKey, hosts.count());
        hosts.append(host);
    }
    return {{"result", hosts}};
}

QJsonObject TauriBridgeHelper::hostDetails(const QJsonObject& payload)
{
    const int hostIndex = hostIndexFromPayload(payload);
    if (hostIndex < 0) {
        return {{"error", "Host was not found."}};
    }

    const FrontendComputer computer = m_Facade.computers()->computerAt(hostIndex);
    return {{"result", QJsonObject{
                           {"name", computer.name},
                           {"address", hostAddress(computer)},
                           {"status", hostStatus(computer)},
                           {"paired", computer.paired},
                           {"running", computer.busy},
                           {"serverVersion", QString()},
                       }}};
}

QJsonObject TauriBridgeHelper::listApps(const QJsonObject& payload)
{
    const int hostIndex = hostIndexFromPayload(payload);
    if (hostIndex < 0) {
        return {{"error", "Host was not found."}};
    }

    const bool showHidden = payload.value("show_hidden").toBool();
    observeAppList(hostIndex, showHidden);
    if (m_ObservedAppList.isNull()) {
        return {{"error", "Unable to create app list."}};
    }

    QJsonArray apps;
    const QVector<FrontendApp> snapshot = m_ObservedAppList->apps();
    for (const FrontendApp& app : snapshot) {
        apps.append(appToJson(app));
    }
    return {{"result", apps}};
}

QJsonObject TauriBridgeHelper::launchApp(const QJsonObject& payload)
{
    const int hostIndex = hostIndexFromPayload(payload);
    if (hostIndex < 0) {
        return {{"error", "Host was not found."}};
    }
    if (m_ActiveSession != nullptr) {
        return {{"error", "A stream session is already active."}};
    }

    QScopedPointer<AppListFacade> appList(m_Facade.createAppList(hostIndex, true));
    const int appIndex = appIndexFromPayload(appList.data(), payload);
    if (appIndex < 0) {
        return {{"error", "App was not found."}};
    }

    const FrontendApp app = appList->appAt(appIndex);
    Session* session = appList->createSessionForApp(appIndex);
    return startSession(session, app.name, app.running, QString::number(hostIndex), payload.value("app_id").toString());
}

QJsonObject TauriBridgeHelper::resumeSession(const QJsonObject& payload)
{
    const int hostIndex = hostIndexFromPayload(payload);
    if (hostIndex < 0) {
        return {{"error", "Host was not found."}};
    }
    if (m_ActiveSession != nullptr) {
        return {{"error", "A stream session is already active."}};
    }

    QScopedPointer<AppListFacade> appList(m_Facade.createAppList(hostIndex, true));
    if (appList.isNull()) {
        return {{"error", "Unable to create app list."}};
    }

    const int runningAppId = appList->getRunningAppId();
    if (runningAppId == 0) {
        return {{"error", tr("This host has no running session to resume.")}};
    }

    QString appName = appList->getRunningAppName();
    if (appName.isEmpty()) {
        appName = tr("the running app");
    }

    Session* session = m_Facade.computers()->createSessionForCurrentGame(hostIndex);
    return startSession(session, appName, true, QString::number(hostIndex), QString::number(runningAppId));
}

QJsonObject TauriBridgeHelper::startSession(Session* session, const QString& appName, bool isResume, const QString& hostId, const QString& appId)
{
    if (session == nullptr) {
        return {{"error", "Unable to start stream: session was not created."}};
    }
    if (m_ActiveSession != nullptr) {
        session->deleteLater();
        return {{"error", "A stream session is already active."}};
    }

    m_ActiveSession = session;
    QObject::connect(session, &Session::readyForDeletion, session, &QObject::deleteLater);
    QObject::connect(session, &Session::destroyed, [this]() {
        m_ActiveSession = nullptr;
    });

    if (m_WindowContextSource.isNull()) {
        m_WindowContextSource.reset(new QWidget());
    }
    m_WindowContext.reset(new QtWidgetWindowContext(m_WindowContextSource.data()));

    m_Facade.system()->waitForAsyncLoad();
    m_Facade.sessions()->setSession(session, appName, isResume, false);
    setControllerNavigationEnabled(false);
    if (!m_Facade.sessions()->initialize(m_WindowContext.data())) {
        const QString error = m_Facade.sessions()->errorText().isEmpty() ?
            tr("Unable to start stream: session initialization failed.") :
            m_Facade.sessions()->errorText();
        m_Facade.sessions()->clearSession();
        session->deleteLater();
        m_ActiveSession = nullptr;
        setControllerNavigationEnabled(true);
        return {{"error", error}};
    }

    m_Facade.sessions()->start();
    const QString message = isResume ? tr("Resume requested for %1.").arg(appName) : tr("Launch requested for %1.").arg(appName);
    return resultWithEvent(
        status(message),
        bridgeEvent("sessionChanged", message, hostId, appId));
}

QJsonObject TauriBridgeHelper::pairHost(const QJsonObject& payload)
{
    const int hostIndex = hostIndexFromPayload(payload);
    if (hostIndex < 0) {
        return {{"error", "Host was not found."}};
    }

    const QString pin = m_Facade.computers()->generatePinString();
    m_Facade.computers()->pairComputer(hostIndex, pin);
    const QString message = tr("Enter PIN %1 on the host to complete pairing.").arg(pin);
    return resultWithEvent(
        QJsonObject{{"pin", pin}, {"message", message}},
        bridgeEvent("hostChanged", message, QString::number(hostIndex)));
}

QJsonObject TauriBridgeHelper::wakeHost(const QJsonObject& payload)
{
    const int hostIndex = hostIndexFromPayload(payload);
    if (hostIndex < 0) {
        return {{"error", "Host was not found."}};
    }

    m_Facade.computers()->wakeComputer(hostIndex);
    const QString message = tr("Wake requested.");
    return resultWithEvent(status(message), bridgeEvent("hostChanged", message, QString::number(hostIndex)));
}

QJsonObject TauriBridgeHelper::renameHost(const QJsonObject& payload)
{
    const int hostIndex = hostIndexFromPayload(payload);
    const QString name = payload.value("name").toString();
    if (hostIndex < 0) {
        return {{"error", "Host was not found."}};
    }
    if (name.isEmpty()) {
        return {{"error", "Host name is required."}};
    }

    m_Facade.computers()->renameComputer(hostIndex, name);
    const QString message = tr("Host renamed.");
    return resultWithEvent(status(message), bridgeEvent("hostChanged", message, QString::number(hostIndex)));
}

QJsonObject TauriBridgeHelper::deleteHost(const QJsonObject& payload)
{
    const int hostIndex = hostIndexFromPayload(payload);
    if (hostIndex < 0) {
        return {{"error", "Host was not found."}};
    }

    m_Facade.computers()->deleteComputer(hostIndex);
    const QString message = tr("Host deleted.");
    return resultWithEvent(status(message), bridgeEvent("hostChanged", message, QString::number(hostIndex)));
}

QJsonObject TauriBridgeHelper::testNetwork(const QJsonObject& payload)
{
    const int hostIndex = hostIndexFromPayload(payload);
    if (hostIndex < 0) {
        return {{"error", "Host was not found."}};
    }

    m_Facade.computers()->testConnectionForComputer(hostIndex);
    const QString message = tr("Network test started.");
    return {{"result", QJsonObject{
        {"result", "unavailable"},
        {"blockedPorts", QJsonArray{}},
        {"message", message},
    }}};
}

QJsonObject TauriBridgeHelper::quitRunningApp(const QJsonObject& payload)
{
    const int hostIndex = hostIndexFromPayload(payload);
    if (hostIndex < 0) {
        return {{"error", "Host was not found."}};
    }

    QScopedPointer<AppListFacade> appList(m_Facade.createAppList(hostIndex, true));
    if (appList.isNull()) {
        return {{"error", "Unable to create app list."}};
    }

    if (m_Facade.sessions()->hasSession()) {
        m_Facade.sessions()->interrupt();
    }
    appList->quitRunningApp();
    const QString message = tr("Quit requested for the running app.");
    return resultWithEvent(status(message), bridgeEvent("sessionChanged", message, QString::number(hostIndex)));
}

QJsonObject TauriBridgeHelper::setAppHidden(const QJsonObject& payload)
{
    const int hostIndex = hostIndexFromPayload(payload);
    if (hostIndex < 0) {
        return {{"error", "Host was not found."}};
    }

    QScopedPointer<AppListFacade> appList(m_Facade.createAppList(hostIndex, true));
    const int appIndex = appIndexFromPayload(appList.data(), payload);
    if (appIndex < 0) {
        return {{"error", "App was not found."}};
    }

    appList->setAppHidden(appIndex, payload.value("hidden").toBool());
    const QString message = tr("App visibility updated.");
    return resultWithEvent(status(message), bridgeEvent("appChanged", message, QString::number(hostIndex), payload.value("app_id").toString()));
}

QJsonObject TauriBridgeHelper::setAppDirectLaunch(const QJsonObject& payload)
{
    const int hostIndex = hostIndexFromPayload(payload);
    if (hostIndex < 0) {
        return {{"error", "Host was not found."}};
    }

    QScopedPointer<AppListFacade> appList(m_Facade.createAppList(hostIndex, true));
    const int appIndex = appIndexFromPayload(appList.data(), payload);
    if (appIndex < 0) {
        return {{"error", "App was not found."}};
    }

    appList->setAppDirectLaunch(appIndex, payload.value("direct_launch").toBool());
    const QString message = tr("Direct-launch app updated.");
    return resultWithEvent(status(message), bridgeEvent("appChanged", message, QString::number(hostIndex), payload.value("app_id").toString()));
}

QJsonObject TauriBridgeHelper::loadSettings()
{
    const FrontendStreamingPreferences preferences = m_Facade.preferences()->preferences();
    return {{"result", QJsonObject{
                           {"width", preferences.width},
                           {"height", preferences.height},
                           {"fps", preferences.fps},
                           {"bitrateKbps", preferences.bitrateKbps},
                           {"enableHdr", preferences.enableHdr},
                           {"gamepadMouse", preferences.gamepadMouse},
                       }}};
}

QJsonObject TauriBridgeHelper::saveSettings(const QJsonObject& payload)
{
    const QJsonObject settings = payload.value("settings").toObject();
    FrontendStreamingPreferences preferences = m_Facade.preferences()->preferences();
    preferences.width = settings.value("width").toInt(preferences.width);
    preferences.height = settings.value("height").toInt(preferences.height);
    preferences.fps = settings.value("fps").toInt(preferences.fps);
    preferences.bitrateKbps = settings.value("bitrateKbps").toInt(preferences.bitrateKbps);
    preferences.enableHdr = settings.value("enableHdr").toBool(preferences.enableHdr);
    preferences.gamepadMouse = settings.value("gamepadMouse").toBool(preferences.gamepadMouse);

    m_Facade.preferences()->applyPreferences(preferences, true);
    const QString message = tr("Settings saved.");
    return resultWithEvent(status(message), bridgeEvent("settingsChanged", message));
}

void TauriBridgeHelper::handleControllerNavigation(ControllerNavigationAction action, bool pressed)
{
    if (!pressed) {
        return;
    }

    QJsonObject event = bridgeEvent("controllerAction", tr("Controller action received."));
    event.insert("controllerAction", controllerActionName(action));
    writeEventFrame(event);
}

void TauriBridgeHelper::handleControllerQuit()
{
    writeEventFrame(bridgeEvent("status", tr("Controller quit requested.")));
}

QJsonObject TauriBridgeHelper::status(const QString& message) const
{
    return {{"message", message}};
}

QJsonObject TauriBridgeHelper::resultWithEvent(const QJsonValue& result, const QJsonObject& event) const
{
    return {{"result", result}, {"events", QJsonArray{event}}};
}

QJsonObject TauriBridgeHelper::bridgeEvent(const QString& kind, const QString& message, const QString& hostId, const QString& appId) const
{
    QJsonObject event{
        {"kind", kind},
        {"message", message},
    };

    if (!hostId.isEmpty()) {
        event.insert("hostId", hostId);
    }
    if (!appId.isEmpty()) {
        event.insert("appId", appId);
    }

    return event;
}

void TauriBridgeHelper::writeEventFrame(const QJsonObject& event) const
{
    const QByteArray frame = QJsonDocument(QJsonObject{{"event", event}}).toJson(QJsonDocument::Compact);
#ifdef Q_OS_WIN
    HANDLE outputHandle = GetStdHandle(STD_OUTPUT_HANDLE);
    if (outputHandle != INVALID_HANDLE_VALUE && outputHandle != nullptr) {
        writeBridgeLine(outputHandle, frame);
    }
#else
    QTextStream output(stdout, QIODevice::WriteOnly);
    output << QString::fromUtf8(frame) << Qt::endl;
    output.flush();
#endif
}

void TauriBridgeHelper::setControllerNavigationEnabled(bool enabled)
{
    if (m_ControllerNavigation.isNull()) {
        return;
    }

    if (enabled) {
        m_ControllerNavigation->notifyWindowFocus(true);
        m_ControllerNavigation->enable();
    }
    else {
        m_ControllerNavigation->disable();
    }
}

void TauriBridgeHelper::observeAppList(int hostIndex, bool showHiddenGames)
{
    m_ObservedAppHostId = QString::number(hostIndex);
    m_ObservedAppList.reset(m_Facade.createAppList(hostIndex, showHiddenGames));
    if (m_ObservedAppList.isNull()) {
        return;
    }

    QObject::connect(m_ObservedAppList.data(), &AppListFacade::appsReset,
                     [this]() {
        if (!m_SuppressFacadeEvents) {
            writeEventFrame(bridgeEvent("appChanged", tr("App list changed."), m_ObservedAppHostId));
        }
    });
    QObject::connect(m_ObservedAppList.data(), &AppListFacade::appChanged,
                     [this](int appIndex) {
        if (m_SuppressFacadeEvents || m_ObservedAppList.isNull()) {
            return;
        }

        const FrontendApp app = m_ObservedAppList->appAt(appIndex);
        writeEventFrame(bridgeEvent("appChanged", tr("App changed."), m_ObservedAppHostId, QString::number(app.appId)));
    });
    QObject::connect(m_ObservedAppList.data(), &AppListFacade::appBoxArtChanged,
                     [this](int appIndex, const QUrl&) {
        if (m_SuppressFacadeEvents || m_ObservedAppList.isNull()) {
            return;
        }

        const FrontendApp app = m_ObservedAppList->appAt(appIndex);
        writeEventFrame(bridgeEvent("appChanged", tr("App box art changed."), m_ObservedAppHostId, QString::number(app.appId)));
    });
    QObject::connect(m_ObservedAppList.data(), &AppListFacade::computerLost,
                     [this]() {
        if (!m_SuppressFacadeEvents) {
            writeEventFrame(bridgeEvent("hostChanged", tr("Selected host is no longer available."), m_ObservedAppHostId));
        }
    });
}

QString TauriBridgeHelper::controllerActionName(ControllerNavigationAction action) const
{
    switch (action) {
    case ControllerNavigationAction::Up:
        return "up";
    case ControllerNavigationAction::Down:
        return "down";
    case ControllerNavigationAction::Left:
        return "left";
    case ControllerNavigationAction::Right:
        return "right";
    case ControllerNavigationAction::Accept:
        return "accept";
    case ControllerNavigationAction::Back:
        return "back";
    case ControllerNavigationAction::ContextMenu:
        return "contextMenu";
    case ControllerNavigationAction::Settings:
        return "settings";
    case ControllerNavigationAction::NextControl:
        return "nextControl";
    case ControllerNavigationAction::PreviousControl:
        return "previousControl";
    case ControllerNavigationAction::ActivateControl:
        return "activateControl";
    }

    return QString();
}

QJsonObject TauriBridgeHelper::hostToJson(const FrontendComputer& computer, int index) const
{
    return {
        {"id", QString::number(index)},
        {"name", computer.name},
        {"address", hostAddress(computer)},
        {"status", hostStatus(computer)},
        {"paired", computer.paired},
        {"running", computer.busy},
    };
}

QJsonObject TauriBridgeHelper::appToJson(const FrontendApp& app) const
{
    return {
        {"id", QString::number(app.appId)},
        {"name", app.name},
        {"hidden", app.hidden},
        {"directLaunch", app.directLaunch},
        {"running", app.running},
    };
}

QString TauriBridgeHelper::hostStatus(const FrontendComputer& computer) const
{
    if (!computer.paired) {
        return "Pairing required";
    }
    return computer.online ? "Online" : "Offline";
}

QString TauriBridgeHelper::hostAddress(const FrontendComputer& computer) const
{
    const QString prefix = tr("Active Address: ");
    const QStringList lines = computer.details.split('\n');
    for (const QString& line : lines) {
        if (line.startsWith(prefix)) {
            const QString address = line.mid(prefix.length());
            return address == "<NULL>" ? QString() : address;
        }
    }
    return QString();
}

int TauriBridgeHelper::hostIndexFromPayload(const QJsonObject& payload)
{
    bool ok = false;
    const int hostIndex = payload.value("host_id").toString().toInt(&ok);
    if (!ok || hostIndex < 0 || hostIndex >= m_Facade.computers()->count()) {
        return -1;
    }
    return hostIndex;
}

int TauriBridgeHelper::appIndexFromPayload(AppListFacade* appList, const QJsonObject& payload) const
{
    if (appList == nullptr) {
        return -1;
    }

    bool ok = false;
    const int appId = payload.value("app_id").toString().toInt(&ok);
    if (!ok) {
        return -1;
    }

    const QVector<FrontendApp> apps = appList->apps();
    for (int i = 0; i < apps.count(); i++) {
        if (apps[i].appId == appId) {
            return i;
        }
    }
    return -1;
}

QJsonObject TauriBridgeHelper::unsupported(const QString& command) const
{
    return {{"error", QString("Bridge command is not implemented by the native helper yet: %1").arg(command)}};
}
