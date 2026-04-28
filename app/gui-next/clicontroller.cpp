#include "clicontroller.h"

#include "backend/computermanager.h"
#include "cli/pair.h"
#include "cli/quitstream.h"
#include "cli/startstream.h"
#include "settings/streamingpreferences.h"
#include "streaming/session.h"

#include <QCoreApplication>
#include <QDebug>
#include <QMessageBox>

GuiNextCliController::GuiNextCliController(QObject* parent)
    : QObject(parent),
      m_SessionCoordinator(this)
{
    m_WindowContextSource.setWindowTitle(tr("Moonlight"));

    connect(&m_SessionCoordinator, &FrontendSessionCoordinator::stageTextChanged,
            this, [](const QString& stageText) {
        qInfo().noquote() << stageText;
    });
    connect(&m_SessionCoordinator, &FrontendSessionCoordinator::launchWarningsChanged,
            this, [](const QStringList& warnings) {
        for (const QString& warning : warnings) {
            qWarning().noquote() << warning;
        }
    });
    connect(&m_SessionCoordinator, &FrontendSessionCoordinator::errorTextChanged,
            this, [this](const QString& errorText) {
        if (!errorText.isEmpty()) {
            m_StreamError = errorText;
            qCritical().noquote() << errorText;
        }
    });
    connect(&m_SessionCoordinator, &FrontendSessionCoordinator::quitApplicationRequested,
            qApp, []() {
        QCoreApplication::exit(0);
    });
    connect(&m_SessionCoordinator, &FrontendSessionCoordinator::sessionFinished,
            this, [this](int) {
        if (!m_StreamError.isEmpty()) {
            QCoreApplication::exit(1);
        }
    });
}

bool GuiNextCliController::start(GlobalCommandLineParser::ParseResult command, const QStringList& arguments)
{
    switch (command) {
    case GlobalCommandLineParser::StreamRequested:
        return startStream(arguments);
    case GlobalCommandLineParser::PairRequested:
        return startPair(arguments);
    case GlobalCommandLineParser::QuitRequested:
        return startQuit(arguments);
    case GlobalCommandLineParser::NormalStartRequested:
    case GlobalCommandLineParser::ListRequested:
        return false;
    }

    return false;
}

bool GuiNextCliController::startStream(const QStringList& arguments)
{
    createComputerManager();

    StreamingPreferences* preferences = StreamingPreferences::get();
    StreamCommandLineParser parser;
    parser.parse(arguments, preferences);

    m_StreamLauncher.reset(new CliStartStream::Launcher(parser.getHost(), parser.getAppName(), preferences, this));
    connect(m_StreamLauncher.data(), &CliStartStream::Launcher::searchingComputer,
            this, []() {
        qInfo("Establishing connection to PC...");
    });
    connect(m_StreamLauncher.data(), &CliStartStream::Launcher::searchingApp,
            this, []() {
        qInfo("Loading app list...");
    });
    connect(m_StreamLauncher.data(), &CliStartStream::Launcher::sessionCreated,
            this, &GuiNextCliController::handleStreamSessionCreated);
    connect(m_StreamLauncher.data(), &CliStartStream::Launcher::failed,
            this, &GuiNextCliController::handleStreamFailure);
    connect(m_StreamLauncher.data(), &CliStartStream::Launcher::appQuitRequired,
            this, &GuiNextCliController::handleAppQuitRequired);

    m_StreamLauncher->execute(m_ComputerManager.data());
    return true;
}

bool GuiNextCliController::startPair(const QStringList& arguments)
{
    createComputerManager();

    PairCommandLineParser parser;
    parser.parse(arguments);

    m_PairLauncher.reset(new CliPair::Launcher(parser.getHost(), parser.getPredefinedPin(), this));
    connect(m_PairLauncher.data(), &CliPair::Launcher::searchingComputer,
            this, []() {
        qInfo("Establishing connection to PC...");
    });
    connect(m_PairLauncher.data(), &CliPair::Launcher::pairing,
            this, [](const QString& pcName, const QString& pin) {
        qInfo().noquote() << QObject::tr("Pairing... Please enter '%1' on %2.").arg(pin, pcName);
    });
    connect(m_PairLauncher.data(), &CliPair::Launcher::failed,
            this, &GuiNextCliController::handlePairingFailure);
    connect(m_PairLauncher.data(), &CliPair::Launcher::success,
            qApp, []() {
        qInfo("Pairing completed successfully");
        QCoreApplication::exit(0);
    });

    m_PairLauncher->execute(m_ComputerManager.data());
    return true;
}

bool GuiNextCliController::startQuit(const QStringList& arguments)
{
    createComputerManager();

    QuitCommandLineParser parser;
    parser.parse(arguments);

    m_QuitLauncher.reset(new CliQuitStream::Launcher(parser.getHost(), this));
    connect(m_QuitLauncher.data(), &CliQuitStream::Launcher::searchingComputer,
            this, []() {
        qInfo("Establishing connection to PC...");
    });
    connect(m_QuitLauncher.data(), &CliQuitStream::Launcher::quittingApp,
            this, []() {
        qInfo("Quitting app...");
    });
    connect(m_QuitLauncher.data(), &CliQuitStream::Launcher::failed,
            this, &GuiNextCliController::handleQuitFailure);

    m_QuitLauncher->execute(m_ComputerManager.data());
    return true;
}

void GuiNextCliController::createComputerManager()
{
    if (m_ComputerManager == nullptr) {
        m_ComputerManager.reset(new ComputerManager(StreamingPreferences::get()));
    }
}

void GuiNextCliController::handleStreamSessionCreated(QString appName, Session* session)
{
    if (session == nullptr) {
        handleStreamFailure(tr("Unable to start stream: session was not created."));
        return;
    }

    m_ActiveSession = session;
    connect(session, &Session::readyForDeletion, session, &QObject::deleteLater);

    m_WindowContext.reset(new QtWidgetWindowContext(&m_WindowContextSource));
    m_SessionCoordinator.setSession(session, appName, false, true);

    if (!m_SessionCoordinator.initialize(m_WindowContext.data())) {
        session->deleteLater();
        QCoreApplication::exit(1);
        return;
    }

    m_SessionCoordinator.start();
}

void GuiNextCliController::handleStreamFailure(QString text)
{
    qCritical().noquote() << text;
    QCoreApplication::exit(1);
}

void GuiNextCliController::handleAppQuitRequired(QString appName)
{
    const QMessageBox::StandardButton result = QMessageBox::question(
        nullptr,
        tr("Quit Running App"),
        tr("Are you sure you want to quit %1? Any unsaved progress will be lost.").arg(appName));

    if (result == QMessageBox::Yes) {
        m_StreamLauncher->quitRunningApp();
    }
    else {
        QCoreApplication::exit(1);
    }
}

void GuiNextCliController::handlePairingFailure(QString text)
{
    qCritical().noquote() << text;
    QCoreApplication::exit(1);
}

void GuiNextCliController::handleQuitFailure(QString text)
{
    qCritical().noquote() << text;
    QCoreApplication::exit(1);
}
