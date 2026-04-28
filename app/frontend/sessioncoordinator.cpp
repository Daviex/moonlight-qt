#include "sessioncoordinator.h"

#include <QDebug>
#include <QVariant>

FrontendSessionCoordinator::FrontendSessionCoordinator(QObject* parent)
    : QObject(parent)
{
}

void FrontendSessionCoordinator::setSession(Session* session, const QString& appName, bool isResume, bool quitAfter)
{
    if (m_Session != nullptr) {
        disconnect(m_Session, nullptr, this, nullptr);
    }

    m_Session = session;
    m_AppName = appName;
    m_IsResume = isResume;
    m_QuitAfter = quitAfter;
    m_ErrorText.clear();
    m_LaunchWarnings.clear();

    setStageText(m_IsResume ? tr("Resuming %1...").arg(m_AppName) :
                              tr("Starting %1...").arg(m_AppName));
    emit errorTextChanged(m_ErrorText);
    emit launchWarningsChanged(m_LaunchWarnings);

    if (m_Session == nullptr) {
        return;
    }

    connect(m_Session, &Session::stageStarting,
            this, &FrontendSessionCoordinator::handleStageStarting);
    connect(m_Session, &Session::stageFailed,
            this, &FrontendSessionCoordinator::handleStageFailed);
    connect(m_Session, &Session::connectionStarted,
            this, &FrontendSessionCoordinator::handleConnectionStarted);
    connect(m_Session, &Session::displayLaunchError,
            this, &FrontendSessionCoordinator::handleDisplayLaunchError);
    connect(m_Session, &Session::quitStarting,
            this, &FrontendSessionCoordinator::handleQuitStarting);
    connect(m_Session, &Session::sessionFinished,
            this, &FrontendSessionCoordinator::handleSessionFinished);
    connect(m_Session, &Session::readyForDeletion,
            this, &FrontendSessionCoordinator::handleReadyForDeletion);
}

bool FrontendSessionCoordinator::initialize(SessionWindowContext* windowContext)
{
    if (m_Session == nullptr) {
        setErrorText(tr("Unable to start stream: session was not created."));
        return false;
    }
    if (windowContext == nullptr) {
        qWarning() << "FrontendSessionCoordinator::initialize called without a window context";
        setErrorText(tr("Unable to start stream: window context was not created."));
        return false;
    }

    bool result = m_Session->initialize(windowContext);
    refreshLaunchWarnings();
    return result;
}

void FrontendSessionCoordinator::start()
{
    if (m_Session == nullptr) {
        setErrorText(tr("Unable to start stream: session was not created."));
        handleSessionFinished(0);
        handleReadyForDeletion();
        return;
    }

    m_Session->start();
}

void FrontendSessionCoordinator::interrupt()
{
    if (m_Session != nullptr) {
        m_Session->interrupt();
    }
}

void FrontendSessionCoordinator::clearSession()
{
    if (m_Session != nullptr) {
        disconnect(m_Session, nullptr, this, nullptr);
    }

    m_Session = nullptr;
}

bool FrontendSessionCoordinator::hasSession() const
{
    return m_Session != nullptr;
}

QString FrontendSessionCoordinator::appName() const
{
    return m_AppName;
}

QString FrontendSessionCoordinator::stageText() const
{
    return m_StageText;
}

QString FrontendSessionCoordinator::errorText() const
{
    return m_ErrorText;
}

QStringList FrontendSessionCoordinator::launchWarnings() const
{
    return m_LaunchWarnings;
}

bool FrontendSessionCoordinator::quitAfter() const
{
    return m_QuitAfter;
}

void FrontendSessionCoordinator::handleStageStarting(QString stage)
{
    setStageText(tr("Starting %1...").arg(stage));
}

void FrontendSessionCoordinator::handleStageFailed(QString stage, int errorCode, QString failingPorts)
{
    QString error = tr("Starting %1 failed: Error %2").arg(stage).arg(errorCode);

    if (!failingPorts.isEmpty()) {
        error += "\n\n" + tr("Check your firewall and port forwarding rules for port(s): %1").arg(failingPorts);
    }

    setErrorText(error);
}

void FrontendSessionCoordinator::handleConnectionStarted()
{
    emit hideUiRequested();
}

void FrontendSessionCoordinator::handleDisplayLaunchError(QString text)
{
    setErrorText(text);
    qWarning() << text;
}

void FrontendSessionCoordinator::handleQuitStarting()
{
    emit quitSegueRequested(m_AppName);
    emit showUiRequested();
}

void FrontendSessionCoordinator::handleSessionFinished(int portTestResult)
{
    if (portTestResult != 0 && portTestResult != -1 && !m_ErrorText.isEmpty()) {
        setErrorText(m_ErrorText + "\n\n" +
                     tr("This PC's Internet connection is blocking Moonlight. Streaming over the Internet may not work while connected to this network."));
    }

    emit sessionFinished(portTestResult);

    if (m_QuitAfter && m_ErrorText.isEmpty()) {
        emit quitApplicationRequested();
    }
    else {
        emit showUiRequested();
    }
}

void FrontendSessionCoordinator::handleReadyForDeletion()
{
    clearSession();
    emit sessionReadyForDeletion();
}

void FrontendSessionCoordinator::setStageText(const QString& stageText)
{
    if (m_StageText == stageText) {
        return;
    }

    m_StageText = stageText;
    emit stageTextChanged(m_StageText);
}

void FrontendSessionCoordinator::setErrorText(const QString& errorText)
{
    if (m_ErrorText == errorText) {
        return;
    }

    m_ErrorText = errorText;
    emit errorTextChanged(m_ErrorText);
}

void FrontendSessionCoordinator::refreshLaunchWarnings()
{
    if (m_Session == nullptr) {
        m_LaunchWarnings.clear();
    }
    else {
        m_LaunchWarnings = m_Session->property("launchWarnings").toStringList();
    }

    emit launchWarningsChanged(m_LaunchWarnings);
}
