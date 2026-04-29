#include "tauribridgehelper.h"

#include "frontend/applistfacade.h"
#include "frontend/sdlcontrollernavigation.h"
#include "settings/streamingpreferences.h"
#include "streaming/qtwidgetwindowcontext.h"

#include <QDesktopServices>
#include <QCoreApplication>
#include <QEventLoop>
#include <QHash>
#include <QJsonArray>
#include <QJsonDocument>
#include <QSet>
#include <QStringList>
#include <QTextStream>
#include <QThread>
#include <QUrl>
#include <QWidget>

#include <cmath>
#ifndef Q_OS_WIN
#include <cerrno>
#include <sys/select.h>
#include <unistd.h>
#endif
#include <limits>

static bool validateIntegerSetting(const QJsonObject& settings,
                                   const QString& key,
                                   const QString& label,
                                   int minimum,
                                   int maximum,
                                   QString& error)
{
    if (!settings.contains(key)) {
        return true;
    }

    const QJsonValue value = settings.value(key);
    if (!value.isDouble()) {
        error = QStringLiteral("%1 must be a number.").arg(label);
        return false;
    }

    const double numericValue = value.toDouble();
    if (!std::isfinite(numericValue) || std::floor(numericValue) != numericValue) {
        error = QStringLiteral("%1 must be a whole number.").arg(label);
        return false;
    }

    if (numericValue < minimum || numericValue > maximum) {
        error = QStringLiteral("%1 must be between %2 and %3.").arg(label).arg(minimum).arg(maximum);
        return false;
    }

    return true;
}

static bool validateRequiredIntegerSetting(const QJsonObject& settings,
                                           const QString& key,
                                           const QString& label,
                                           int minimum,
                                           int maximum,
                                           QString& error)
{
    if (!settings.contains(key)) {
        error = QStringLiteral("%1 is required.").arg(label);
        return false;
    }

    return validateIntegerSetting(settings, key, label, minimum, maximum, error);
}

static bool validateBooleanSetting(const QJsonObject& settings,
                                   const QString& key,
                                   const QString& label,
                                   QString& error)
{
    if (!settings.contains(key)) {
        return true;
    }

    if (!settings.value(key).isBool()) {
        error = QStringLiteral("%1 must be a boolean.").arg(label);
        return false;
    }

    return true;
}

static bool validateRequiredBooleanSetting(const QJsonObject& settings,
                                           const QString& key,
                                           const QString& label,
                                           QString& error)
{
    if (!settings.contains(key)) {
        error = QStringLiteral("%1 is required.").arg(label);
        return false;
    }

    return validateBooleanSetting(settings, key, label, error);
}

static bool parseRequiredNumericString(const QJsonObject& payload,
                                       const QString& key,
                                       const QString& label,
                                       int& value,
                                       QString& error)
{
    if (!payload.contains(key)) {
        error = QStringLiteral("%1 is required.").arg(label);
        return false;
    }

    const QJsonValue jsonValue = payload.value(key);
    if (!jsonValue.isString() || jsonValue.toString().isEmpty()) {
        error = QStringLiteral("%1 must be a numeric string.").arg(label);
        return false;
    }

    bool ok = false;
    value = jsonValue.toString().toInt(&ok);
    if (!ok || value < 0) {
        error = QStringLiteral("%1 must be a non-negative integer.").arg(label);
        return false;
    }

    return true;
}

static bool parseRequiredString(const QJsonObject& payload,
                                const QString& key,
                                const QString& label,
                                QString& value,
                                QString& error)
{
    if (!payload.contains(key)) {
        error = QStringLiteral("%1 is required.").arg(label);
        return false;
    }

    const QJsonValue jsonValue = payload.value(key);
    if (!jsonValue.isString()) {
        error = QStringLiteral("%1 must be a string.").arg(label);
        return false;
    }

    value = jsonValue.toString();
    if (value.isEmpty()) {
        error = QStringLiteral("%1 is required.").arg(label);
        return false;
    }

    return true;
}

static bool validateStreamingBooleanSettings(const QJsonObject& settings, QString& error)
{
    return validateBooleanSetting(settings, QStringLiteral("unlockBitrate"), QStringLiteral("Unlock bitrate"), error) &&
        validateBooleanSetting(settings, QStringLiteral("autoAdjustBitrate"), QStringLiteral("Auto-adjust bitrate"), error) &&
        validateBooleanSetting(settings, QStringLiteral("enableVsync"), QStringLiteral("V-Sync"), error) &&
        validateBooleanSetting(settings, QStringLiteral("gameOptimizations"), QStringLiteral("Game optimizations"), error) &&
        validateBooleanSetting(settings, QStringLiteral("playAudioOnHost"), QStringLiteral("Play audio on host"), error) &&
        validateBooleanSetting(settings, QStringLiteral("multiController"), QStringLiteral("Multiple controllers"), error) &&
        validateBooleanSetting(settings, QStringLiteral("enableMdns"), QStringLiteral("mDNS discovery"), error) &&
        validateBooleanSetting(settings, QStringLiteral("quitAppAfter"), QStringLiteral("Quit app after stream"), error) &&
        validateBooleanSetting(settings, QStringLiteral("absoluteMouseMode"), QStringLiteral("Absolute mouse mode"), error) &&
        validateBooleanSetting(settings, QStringLiteral("absoluteTouchMode"), QStringLiteral("Absolute touch mode"), error) &&
        validateBooleanSetting(settings, QStringLiteral("framePacing"), QStringLiteral("Frame pacing"), error) &&
        validateBooleanSetting(settings, QStringLiteral("connectionWarnings"), QStringLiteral("Connection warnings"), error) &&
        validateBooleanSetting(settings, QStringLiteral("configurationWarnings"), QStringLiteral("Configuration warnings"), error) &&
        validateBooleanSetting(settings, QStringLiteral("richPresence"), QStringLiteral("Rich presence"), error) &&
        validateBooleanSetting(settings, QStringLiteral("enableHdr"), QStringLiteral("HDR"), error) &&
        validateBooleanSetting(settings, QStringLiteral("gamepadMouse"), QStringLiteral("Gamepad mouse"), error) &&
        validateBooleanSetting(settings, QStringLiteral("detectNetworkBlocking"), QStringLiteral("Network blocking detection"), error) &&
        validateBooleanSetting(settings, QStringLiteral("showPerformanceOverlay"), QStringLiteral("Performance overlay"), error) &&
        validateBooleanSetting(settings, QStringLiteral("swapMouseButtons"), QStringLiteral("Swap mouse buttons"), error) &&
        validateBooleanSetting(settings, QStringLiteral("muteOnFocusLoss"), QStringLiteral("Mute on focus loss"), error) &&
        validateBooleanSetting(settings, QStringLiteral("backgroundGamepad"), QStringLiteral("Background gamepad"), error) &&
        validateBooleanSetting(settings, QStringLiteral("reverseScrollDirection"), QStringLiteral("Reverse scroll direction"), error) &&
        validateBooleanSetting(settings, QStringLiteral("swapFaceButtons"), QStringLiteral("Swap face buttons"), error) &&
        validateBooleanSetting(settings, QStringLiteral("keepAwake"), QStringLiteral("Keep awake"), error) &&
        validateBooleanSetting(settings, QStringLiteral("enableYUV444"), QStringLiteral("YUV 4:4:4"), error);
}

static bool validateStreamingSettings(const QJsonObject& settings, QString& error)
{
    return validateIntegerSetting(settings, QStringLiteral("width"), QStringLiteral("Width"), 256, 8192, error) &&
        validateIntegerSetting(settings, QStringLiteral("height"), QStringLiteral("Height"), 256, 8192, error) &&
        validateIntegerSetting(settings, QStringLiteral("fps"), QStringLiteral("FPS"), 10, 9999, error) &&
        validateIntegerSetting(settings, QStringLiteral("bitrateKbps"), QStringLiteral("Bitrate"), 500, 500000, error) &&
        validateIntegerSetting(settings, QStringLiteral("packetSize"), QStringLiteral("Packet size"), 0, 9000, error) &&
        validateIntegerSetting(settings, QStringLiteral("audioConfig"), QStringLiteral("Audio configuration"), 0, 2, error) &&
        validateIntegerSetting(settings, QStringLiteral("videoCodecConfig"), QStringLiteral("Video codec"), 0, 4, error) &&
        validateIntegerSetting(settings, QStringLiteral("videoDecoderSelection"), QStringLiteral("Video decoder"), 0, 2, error) &&
        validateIntegerSetting(settings, QStringLiteral("windowMode"), QStringLiteral("Stream window mode"), 0, 2, error) &&
        validateIntegerSetting(settings, QStringLiteral("uiDisplayMode"), QStringLiteral("UI startup mode"), 0, 2, error) &&
        validateIntegerSetting(settings, QStringLiteral("language"), QStringLiteral("Language"),
                               static_cast<int>(StreamingPreferences::LANG_AUTO),
                               static_cast<int>(StreamingPreferences::LANG_TA), error) &&
        validateIntegerSetting(settings, QStringLiteral("captureSysKeysMode"), QStringLiteral("Capture system keys mode"), 0, 2, error) &&
        validateStreamingBooleanSettings(settings, error);
}

static bool validateDefaultBitratePayload(const QJsonObject& payload, QString& error)
{
    if (!validateRequiredIntegerSetting(payload, QStringLiteral("width"), QStringLiteral("Width"), 256, 8192, error) ||
        !validateRequiredIntegerSetting(payload, QStringLiteral("height"), QStringLiteral("Height"), 256, 8192, error) ||
        !validateRequiredIntegerSetting(payload, QStringLiteral("fps"), QStringLiteral("FPS"), 10, 9999, error)) {
        return false;
    }

    if (payload.contains(QStringLiteral("yuv444")) && !payload.value(QStringLiteral("yuv444")).isBool()) {
        error = QStringLiteral("YUV444 must be a boolean.");
        return false;
    }

    return true;
}

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

#ifndef Q_OS_WIN
static bool readBridgeLine(QByteArray& line)
{
    line.clear();

    char ch = 0;
    while (true) {
        fd_set readSet;
        FD_ZERO(&readSet);
        FD_SET(STDIN_FILENO, &readSet);

        timeval timeout;
        timeout.tv_sec = 0;
        timeout.tv_usec = 10000;

        int ready = select(STDIN_FILENO + 1, &readSet, nullptr, nullptr, &timeout);
        if (ready < 0) {
            if (errno == EINTR) {
                continue;
            }
            return !line.isEmpty();
        }
        if (ready == 0) {
            QCoreApplication::processEvents(QEventLoop::AllEvents, 10);
            continue;
        }

        const ssize_t bytesRead = read(STDIN_FILENO, &ch, 1);
        if (bytesRead <= 0) {
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
    QObject::connect(&m_ComputerManager, &ComputerManager::computerAddCompleted,
                     [this](const QVariant& success, const QVariant& detectedPortBlocking) {
        if (!m_SuppressFacadeEvents) {
            QString message;
            if (success.toBool()) {
                message = tr("Host add completed.");
            }
            else if (detectedPortBlocking.toBool()) {
                message = tr("Failed to add host. The network may be blocking Moonlight ports.");
            }
            else {
                message = tr("Failed to add host. Check the IP address and make sure Sunshine or GameStream is running.");
            }
            writeEventFrame(bridgeEvent(success.toBool() ? "hostChanged" : "status", message));
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
    QObject::connect(m_Facade.updates(), &UpdateFacade::updateAvailable,
                     [this](const QString& newVersion, const QString& url) {
        QJsonObject event = bridgeEvent(
            "updateAvailable",
            tr("Update available for Moonlight: Version %1").arg(newVersion));
        event.insert("updateVersion", newVersion);
        event.insert("updateUrl", url);
        writeEventFrame(event);
    });

    m_ComputerManager.startPolling();
    m_Facade.updates()->start();
}

int TauriBridgeHelper::run()
{
    auto processLine = [this](const QByteArray& rawLine) -> QVector<QByteArray> {
        const QString line = QString::fromUtf8(rawLine).trimmed();
        if (line.isEmpty()) {
            return {};
        }

        QJsonObject response;
        QJsonParseError parseError;
        const QJsonDocument requestDocument = QJsonDocument::fromJson(line.toUtf8(), &parseError);
        if (parseError.error != QJsonParseError::NoError || !requestDocument.isObject()) {
            response.insert("id", QJsonValue::Null);
            response.insert("error", "Invalid bridge request JSON.");
            return {QJsonDocument(response).toJson(QJsonDocument::Compact)};
        }

        const QJsonObject request = requestDocument.object();
        const QJsonValue requestIdValue = request.value("id");
        if (!requestIdValue.isDouble()) {
            response.insert("id", QJsonValue::Null);
            response.insert("error", "Bridge request id must be a number.");
            return {QJsonDocument(response).toJson(QJsonDocument::Compact)};
        }

        const double requestIdNumber = requestIdValue.toDouble();
        if (!std::isfinite(requestIdNumber) ||
            std::floor(requestIdNumber) != requestIdNumber ||
            requestIdNumber < 0 ||
            requestIdNumber > std::numeric_limits<int>::max()) {
            response.insert("id", QJsonValue::Null);
            response.insert("error", "Bridge request id must be a non-negative integer.");
            return {QJsonDocument(response).toJson(QJsonDocument::Compact)};
        }

        const int requestId = static_cast<int>(requestIdNumber);
        response.insert("id", requestId);

        QJsonArray events;
        if (!request.value("command").isObject()) {
            response.insert("error", "Bridge request command must be an object.");
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
    QTextStream output(stdout, QIODevice::WriteOnly);

    QByteArray line;
    while (readBridgeLine(line)) {
        const QVector<QByteArray> responses = processLine(line);
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
    if (!command.value("name").isString() || command.value("name").toString().isEmpty()) {
        return {{"error", "Bridge command name is required."}};
    }
    if (command.contains("payload") && !command.value("payload").isObject()) {
        return {{"error", "Bridge command payload must be an object."}};
    }

    const QString commandName = command.value("name").toString();
    const QJsonObject payload = command.value("payload").toObject();

    if (commandName == "list_hosts") {
        return listHosts();
    }
    if (commandName == "add_host") {
        QString validationError;
        QString address;
        if (!parseRequiredString(payload, QStringLiteral("address"), QStringLiteral("Host address"), address, validationError)) {
            return {{"error", validationError}};
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
    if (commandName == "default_bitrate") {
        return defaultBitrate(payload);
    }
    if (commandName == "system_info") {
        return systemInfo();
    }
    if (commandName == "open_url") {
        return openUrl(payload);
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
    QString validationError;
    const int hostIndex = hostIndexFromPayload(payload, &validationError);
    if (hostIndex < 0) {
        return {{"error", validationError}};
    }

    const FrontendComputer computer = m_Facade.computers()->computerAt(hostIndex);
    const bool online = computer.online;
    return {{"result", QJsonObject{
                            {"name", computer.name},
                            {"address", hostAddress(computer)},
                            {"status", hostStatus(computer)},
                            {"paired", computer.paired},
                            {"running", computer.busy},
                            {"wakeable", computer.wakeable},
                            {"serverSupported", computer.serverSupported},
                            {"uuid", computer.uuid},
                            {"localAddress", computer.localAddress == QStringLiteral("<NULL>") ? QString() : computer.localAddress},
                            {"remoteAddress", computer.remoteAddress == QStringLiteral("<NULL>") ? QString() : computer.remoteAddress},
                            {"ipv6Address", computer.ipv6Address == QStringLiteral("<NULL>") ? QString() : computer.ipv6Address},
                            {"manualAddress", computer.manualAddress == QStringLiteral("<NULL>") ? QString() : computer.manualAddress},
                            {"macAddress", computer.macAddress},
                            {"pairState", computer.pairState},
                            {"runningGameId", online ? computer.runningGameId : 0},
                            {"httpsPort", online ? computer.httpsPort : 0},
                            {"appVersion", computer.appVersion},
                            {"gfeVersion", computer.gfeVersion},
                            {"serverVersion", computer.appVersion.isEmpty() ? computer.gfeVersion : computer.appVersion},
                            {"gpuModel", computer.gpuModel},
                            {"details", computer.details},
                        }}};
}

QJsonObject TauriBridgeHelper::listApps(const QJsonObject& payload)
{
    QString validationError;
    const int hostIndex = hostIndexFromPayload(payload, &validationError);
    if (hostIndex < 0) {
        return {{"error", validationError}};
    }
    if (payload.contains(QStringLiteral("show_hidden")) &&
        !validateBooleanSetting(payload, QStringLiteral("show_hidden"), QStringLiteral("Show hidden apps"), validationError)) {
        return {{"error", validationError}};
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
    QString validationError;
    const int hostIndex = hostIndexFromPayload(payload, &validationError);
    if (hostIndex < 0) {
        return {{"error", validationError}};
    }
    if (m_ActiveSession != nullptr) {
        return {{"error", "A stream session is already active."}};
    }

    QScopedPointer<AppListFacade> appList(m_Facade.createAppList(hostIndex, true));
    const int appIndex = appIndexFromPayload(appList.data(), payload, &validationError);
    if (appIndex < 0) {
        return {{"error", validationError}};
    }

    const FrontendApp app = appList->appAt(appIndex);
    Session* session = appList->createSessionForApp(appIndex);
    return startSession(session, app.name, app.running, QString::number(hostIndex), payload.value("app_id").toString());
}

QJsonObject TauriBridgeHelper::resumeSession(const QJsonObject& payload)
{
    QString validationError;
    const int hostIndex = hostIndexFromPayload(payload, &validationError);
    if (hostIndex < 0) {
        return {{"error", validationError}};
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
    QString validationError;
    const int hostIndex = hostIndexFromPayload(payload, &validationError);
    if (hostIndex < 0) {
        return {{"error", validationError}};
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
    QString validationError;
    const int hostIndex = hostIndexFromPayload(payload, &validationError);
    if (hostIndex < 0) {
        return {{"error", validationError}};
    }

    m_Facade.computers()->wakeComputer(hostIndex);
    const QString message = tr("Wake requested.");
    return resultWithEvent(status(message), bridgeEvent("hostChanged", message, QString::number(hostIndex)));
}

QJsonObject TauriBridgeHelper::renameHost(const QJsonObject& payload)
{
    QString validationError;
    const int hostIndex = hostIndexFromPayload(payload, &validationError);
    if (hostIndex < 0) {
        return {{"error", validationError}};
    }
    QString name;
    if (!parseRequiredString(payload, QStringLiteral("name"), QStringLiteral("Host name"), name, validationError)) {
        return {{"error", validationError}};
    }

    m_Facade.computers()->renameComputer(hostIndex, name);
    const QString message = tr("Host renamed.");
    return resultWithEvent(status(message), bridgeEvent("hostChanged", message, QString::number(hostIndex)));
}

QJsonObject TauriBridgeHelper::deleteHost(const QJsonObject& payload)
{
    QString validationError;
    const int hostIndex = hostIndexFromPayload(payload, &validationError);
    if (hostIndex < 0) {
        return {{"error", validationError}};
    }

    m_Facade.computers()->deleteComputer(hostIndex);
    const QString message = tr("Host deleted.");
    return resultWithEvent(status(message), bridgeEvent("hostChanged", message, QString::number(hostIndex)));
}

QJsonObject TauriBridgeHelper::testNetwork(const QJsonObject& payload)
{
    QString validationError;
    const int hostIndex = hostIndexFromPayload(payload, &validationError);
    if (hostIndex < 0) {
        return {{"error", validationError}};
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
    QString validationError;
    const int hostIndex = hostIndexFromPayload(payload, &validationError);
    if (hostIndex < 0) {
        return {{"error", validationError}};
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
    QString validationError;
    const int hostIndex = hostIndexFromPayload(payload, &validationError);
    if (hostIndex < 0) {
        return {{"error", validationError}};
    }

    QScopedPointer<AppListFacade> appList(m_Facade.createAppList(hostIndex, true));
    const int appIndex = appIndexFromPayload(appList.data(), payload, &validationError);
    if (appIndex < 0) {
        return {{"error", validationError}};
    }
    if (!validateRequiredBooleanSetting(payload, QStringLiteral("hidden"), QStringLiteral("Hidden"), validationError)) {
        return {{"error", validationError}};
    }

    appList->setAppHidden(appIndex, payload.value("hidden").toBool());
    const QString message = tr("App visibility updated.");
    return resultWithEvent(status(message), bridgeEvent("appChanged", message, QString::number(hostIndex), payload.value("app_id").toString()));
}

QJsonObject TauriBridgeHelper::setAppDirectLaunch(const QJsonObject& payload)
{
    QString validationError;
    const int hostIndex = hostIndexFromPayload(payload, &validationError);
    if (hostIndex < 0) {
        return {{"error", validationError}};
    }

    QScopedPointer<AppListFacade> appList(m_Facade.createAppList(hostIndex, true));
    const int appIndex = appIndexFromPayload(appList.data(), payload, &validationError);
    if (appIndex < 0) {
        return {{"error", validationError}};
    }
    if (!validateRequiredBooleanSetting(payload, QStringLiteral("direct_launch"), QStringLiteral("Direct launch"), validationError)) {
        return {{"error", validationError}};
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
                           {"packetSize", preferences.packetSize},
                           {"audioConfig", preferences.audioConfig},
                           {"videoCodecConfig", preferences.videoCodecConfig},
                            {"videoDecoderSelection", preferences.videoDecoderSelection},
                            {"windowMode", preferences.windowMode},
                            {"uiDisplayMode", preferences.uiDisplayMode},
                            {"language", preferences.language},
                            {"captureSysKeysMode", preferences.captureSysKeysMode},
                            {"unlockBitrate", preferences.unlockBitrate},
                           {"autoAdjustBitrate", preferences.autoAdjustBitrate},
                           {"enableVsync", preferences.enableVsync},
                           {"gameOptimizations", preferences.gameOptimizations},
                           {"playAudioOnHost", preferences.playAudioOnHost},
                           {"multiController", preferences.multiController},
                           {"enableMdns", preferences.enableMdns},
                           {"quitAppAfter", preferences.quitAppAfter},
                           {"absoluteMouseMode", preferences.absoluteMouseMode},
                           {"absoluteTouchMode", preferences.absoluteTouchMode},
                           {"framePacing", preferences.framePacing},
                           {"connectionWarnings", preferences.connectionWarnings},
                           {"configurationWarnings", preferences.configurationWarnings},
                           {"richPresence", preferences.richPresence},
                           {"enableHdr", preferences.enableHdr},
                           {"gamepadMouse", preferences.gamepadMouse},
                           {"detectNetworkBlocking", preferences.detectNetworkBlocking},
                           {"showPerformanceOverlay", preferences.showPerformanceOverlay},
                           {"swapMouseButtons", preferences.swapMouseButtons},
                           {"muteOnFocusLoss", preferences.muteOnFocusLoss},
                           {"backgroundGamepad", preferences.backgroundGamepad},
                           {"reverseScrollDirection", preferences.reverseScrollDirection},
                           {"swapFaceButtons", preferences.swapFaceButtons},
                           {"keepAwake", preferences.keepAwake},
                           {"enableYUV444", preferences.enableYUV444},
                        }}};
}

QJsonObject TauriBridgeHelper::saveSettings(const QJsonObject& payload)
{
    const QJsonValue settingsValue = payload.value("settings");
    if (!settingsValue.isObject()) {
        return {{"error", "Settings payload must be an object."}};
    }

    const QJsonObject settings = settingsValue.toObject();
    QString validationError;
    if (!validateStreamingSettings(settings, validationError)) {
        return {{"error", validationError}};
    }

    FrontendStreamingPreferences preferences = m_Facade.preferences()->preferences();
    preferences.width = settings.value("width").toInt(preferences.width);
    preferences.height = settings.value("height").toInt(preferences.height);
    preferences.fps = settings.value("fps").toInt(preferences.fps);
    preferences.bitrateKbps = settings.value("bitrateKbps").toInt(preferences.bitrateKbps);
    preferences.packetSize = settings.value("packetSize").toInt(preferences.packetSize);
    preferences.audioConfig = settings.value("audioConfig").toInt(preferences.audioConfig);
    preferences.videoCodecConfig = settings.value("videoCodecConfig").toInt(preferences.videoCodecConfig);
    preferences.videoDecoderSelection = settings.value("videoDecoderSelection").toInt(preferences.videoDecoderSelection);
    preferences.windowMode = settings.value("windowMode").toInt(preferences.windowMode);
    preferences.uiDisplayMode = settings.value("uiDisplayMode").toInt(preferences.uiDisplayMode);
    preferences.language = settings.value("language").toInt(preferences.language);
    preferences.captureSysKeysMode = settings.value("captureSysKeysMode").toInt(preferences.captureSysKeysMode);
    preferences.unlockBitrate = settings.value("unlockBitrate").toBool(preferences.unlockBitrate);
    preferences.autoAdjustBitrate = settings.value("autoAdjustBitrate").toBool(preferences.autoAdjustBitrate);
    preferences.enableVsync = settings.value("enableVsync").toBool(preferences.enableVsync);
    preferences.gameOptimizations = settings.value("gameOptimizations").toBool(preferences.gameOptimizations);
    preferences.playAudioOnHost = settings.value("playAudioOnHost").toBool(preferences.playAudioOnHost);
    preferences.multiController = settings.value("multiController").toBool(preferences.multiController);
    preferences.enableMdns = settings.value("enableMdns").toBool(preferences.enableMdns);
    preferences.quitAppAfter = settings.value("quitAppAfter").toBool(preferences.quitAppAfter);
    preferences.absoluteMouseMode = settings.value("absoluteMouseMode").toBool(preferences.absoluteMouseMode);
    preferences.absoluteTouchMode = settings.value("absoluteTouchMode").toBool(preferences.absoluteTouchMode);
    preferences.framePacing = settings.value("framePacing").toBool(preferences.framePacing);
    preferences.connectionWarnings = settings.value("connectionWarnings").toBool(preferences.connectionWarnings);
    preferences.configurationWarnings = settings.value("configurationWarnings").toBool(preferences.configurationWarnings);
    preferences.richPresence = settings.value("richPresence").toBool(preferences.richPresence);
    preferences.enableHdr = settings.value("enableHdr").toBool(preferences.enableHdr);
    preferences.gamepadMouse = settings.value("gamepadMouse").toBool(preferences.gamepadMouse);
    preferences.detectNetworkBlocking = settings.value("detectNetworkBlocking").toBool(preferences.detectNetworkBlocking);
    preferences.showPerformanceOverlay = settings.value("showPerformanceOverlay").toBool(preferences.showPerformanceOverlay);
    preferences.swapMouseButtons = settings.value("swapMouseButtons").toBool(preferences.swapMouseButtons);
    preferences.muteOnFocusLoss = settings.value("muteOnFocusLoss").toBool(preferences.muteOnFocusLoss);
    preferences.backgroundGamepad = settings.value("backgroundGamepad").toBool(preferences.backgroundGamepad);
    preferences.reverseScrollDirection = settings.value("reverseScrollDirection").toBool(preferences.reverseScrollDirection);
    preferences.swapFaceButtons = settings.value("swapFaceButtons").toBool(preferences.swapFaceButtons);
    preferences.keepAwake = settings.value("keepAwake").toBool(preferences.keepAwake);
    preferences.enableYUV444 = settings.value("enableYUV444").toBool(preferences.enableYUV444);

    m_Facade.preferences()->applyPreferences(preferences, true);
    const QString message = tr("Settings saved.");
    return resultWithEvent(status(message), bridgeEvent("settingsChanged", message));
}

QJsonObject TauriBridgeHelper::defaultBitrate(const QJsonObject& payload)
{
    QString validationError;
    if (!validateDefaultBitratePayload(payload, validationError)) {
        return {{"error", validationError}};
    }

    const int width = payload.value("width").toInt();
    const int height = payload.value("height").toInt();
    const int fps = payload.value("fps").toInt();
    const bool yuv444 = payload.value("yuv444").toBool();

    return {{"result", m_Facade.preferences()->getDefaultBitrate(width, height, fps, yuv444)}};
}

QJsonObject TauriBridgeHelper::systemInfo()
{
    m_Facade.system()->startAsyncLoad();
    m_Facade.system()->waitForAsyncLoad();
    m_Facade.system()->refreshDisplays();

    const FrontendSystemProperties system = m_Facade.system()->properties();
    QJsonArray displays;
    for (const FrontendDisplayInfo& display : system.displays) {
        displays.append(QJsonObject{
            {"nativeWidth", display.nativeResolution.width()},
            {"nativeHeight", display.nativeResolution.height()},
            {"safeAreaWidth", display.safeAreaResolution.width()},
            {"safeAreaHeight", display.safeAreaResolution.height()},
            {"refreshRate", display.refreshRate},
        });
    }

    return {{"result", QJsonObject{
        {"version", system.versionString},
        {"friendlyNativeArchName", system.friendlyNativeArchName},
        {"isRunningWayland", system.isRunningWayland},
        {"isRunningXWayland", system.isRunningXWayland},
        {"isWow64", system.isWow64},
        {"hasDesktopEnvironment", system.hasDesktopEnvironment},
        {"hasBrowser", system.hasBrowser},
        {"hasDiscordIntegration", system.hasDiscordIntegration},
        {"usesMaterial3Theme", system.usesMaterial3Theme},
        {"hasHardwareAcceleration", system.hasHardwareAcceleration},
        {"rendererAlwaysFullScreen", system.rendererAlwaysFullScreen},
        {"maximumResolutionWidth", system.maximumResolution.width()},
        {"maximumResolutionHeight", system.maximumResolution.height()},
        {"supportsHdr", system.supportsHdr},
        {"unmappedGamepads", system.unmappedGamepads},
        {"displays", displays},
    }}};
}

QJsonObject TauriBridgeHelper::openUrl(const QJsonObject& payload)
{
    QString validationError;
    QString url;
    if (!parseRequiredString(payload, QStringLiteral("url"), QStringLiteral("URL"), url, validationError)) {
        return {{"error", validationError}};
    }

    const QUrl targetUrl(url, QUrl::StrictMode);
    const QString scheme = targetUrl.scheme().toLower();
    if (!targetUrl.isValid() || (scheme != QStringLiteral("http") && scheme != QStringLiteral("https"))) {
        return {{"error", tr("Only HTTP and HTTPS URLs can be opened from the Tauri bridge.")}};
    }

    m_Facade.system()->startAsyncLoad();
    m_Facade.system()->waitForAsyncLoad();
    if (!m_Facade.system()->properties().hasBrowser) {
        return {{"error", tr("No web browser is available to open %1.").arg(targetUrl.toString())}};
    }

    if (!QDesktopServices::openUrl(targetUrl)) {
        return {{"error", tr("Failed to open %1.").arg(targetUrl.toString())}};
    }

    return {{"message", tr("Opened %1.").arg(targetUrl.toString())}};
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
        {"wakeable", computer.wakeable},
        {"serverSupported", computer.serverSupported},
    };
}

QJsonObject TauriBridgeHelper::appToJson(const FrontendApp& app) const
{
    return {
        {"id", QString::number(app.appId)},
        {"name", app.name},
        {"boxArtUrl", app.boxArt.toString()},
        {"hidden", app.hidden},
        {"directLaunch", app.directLaunch},
        {"running", app.running},
        {"appCollectorGame", app.appCollectorGame},
    };
}

QString TauriBridgeHelper::hostStatus(const FrontendComputer& computer) const
{
    if (!computer.online) {
        return "Offline";
    }
    if (!computer.paired) {
        return "Pairing required";
    }
    return "Online";
}

QString TauriBridgeHelper::hostAddress(const FrontendComputer& computer) const
{
    if (!computer.activeAddress.isEmpty() && computer.activeAddress != QStringLiteral("<NULL>")) {
        return computer.activeAddress;
    }

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

int TauriBridgeHelper::hostIndexFromPayload(const QJsonObject& payload, QString* error)
{
    QString validationError;
    int hostIndex = -1;
    if (!parseRequiredNumericString(payload, QStringLiteral("host_id"), QStringLiteral("Host ID"), hostIndex, validationError)) {
        if (error != nullptr) {
            *error = validationError;
        }
        return -1;
    }

    if (hostIndex >= m_Facade.computers()->count()) {
        if (error != nullptr) {
            *error = QStringLiteral("Host was not found.");
        }
        return -1;
    }
    return hostIndex;
}

int TauriBridgeHelper::appIndexFromPayload(AppListFacade* appList, const QJsonObject& payload, QString* error) const
{
    if (appList == nullptr) {
        if (error != nullptr) {
            *error = QStringLiteral("Unable to create app list.");
        }
        return -1;
    }

    QString validationError;
    int appId = -1;
    if (!parseRequiredNumericString(payload, QStringLiteral("app_id"), QStringLiteral("App ID"), appId, validationError)) {
        if (error != nullptr) {
            *error = validationError;
        }
        return -1;
    }

    const QVector<FrontendApp> apps = appList->apps();
    for (int i = 0; i < apps.count(); i++) {
        if (apps[i].appId == appId) {
            return i;
        }
    }
    if (error != nullptr) {
        *error = QStringLiteral("App was not found.");
    }
    return -1;
}

QJsonObject TauriBridgeHelper::unsupported(const QString& command) const
{
    return {{"error", QString("Bridge command is not implemented by the native helper yet: %1").arg(command)}};
}
