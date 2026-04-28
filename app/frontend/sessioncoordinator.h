#pragma once

#include "streaming/session.h"

#include <QObject>
#include <QPointer>
#include <QStringList>

class FrontendSessionCoordinator : public QObject
{
    Q_OBJECT

public:
    explicit FrontendSessionCoordinator(QObject* parent = nullptr);

    void setSession(Session* session, const QString& appName, bool isResume, bool quitAfter);
    bool initialize(SessionWindowContext* windowContext);
    void start();
    void interrupt();
    void clearSession();

    bool hasSession() const;
    QString appName() const;
    QString stageText() const;
    QString errorText() const;
    QStringList launchWarnings() const;
    bool quitAfter() const;

signals:
    void stageTextChanged(QString stageText);
    void errorTextChanged(QString errorText);
    void launchWarningsChanged(QStringList launchWarnings);
    void hideUiRequested();
    void showUiRequested();
    void quitSegueRequested(QString appName);
    void quitApplicationRequested();
    void sessionFinished(int portTestResult);
    void sessionReadyForDeletion();

private slots:
    void handleStageStarting(QString stage);
    void handleStageFailed(QString stage, int errorCode, QString failingPorts);
    void handleConnectionStarted();
    void handleDisplayLaunchError(QString text);
    void handleQuitStarting();
    void handleSessionFinished(int portTestResult);
    void handleReadyForDeletion();

private:
    void setStageText(const QString& stageText);
    void setErrorText(const QString& errorText);
    void refreshLaunchWarnings();

    QPointer<Session> m_Session;
    QString m_AppName;
    QString m_StageText;
    QString m_ErrorText;
    QStringList m_LaunchWarnings;
    bool m_IsResume = false;
    bool m_QuitAfter = false;
};
