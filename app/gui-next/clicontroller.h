#pragma once

#include "cli/commandlineparser.h"
#include "cli/pair.h"
#include "cli/quitstream.h"
#include "cli/startstream.h"
#include "frontend/sessioncoordinator.h"
#include "streaming/qtwidgetwindowcontext.h"

#include <QObject>
#include <QPointer>
#include <QScopedPointer>
#include <QStringList>
#include <QWidget>

class ComputerManager;
class Session;
class StreamingPreferences;

class GuiNextCliController : public QObject
{
    Q_OBJECT

public:
    explicit GuiNextCliController(QObject* parent = nullptr);

    bool start(GlobalCommandLineParser::ParseResult command, const QStringList& arguments);

private slots:
    void handleStreamSessionCreated(QString appName, Session* session);
    void handleStreamFailure(QString text);
    void handleAppQuitRequired(QString appName);
    void handlePairingFailure(QString text);
    void handleQuitFailure(QString text);

private:
    bool startStream(const QStringList& arguments);
    bool startPair(const QStringList& arguments);
    bool startQuit(const QStringList& arguments);
    void createComputerManager();

    QScopedPointer<ComputerManager> m_ComputerManager;
    QScopedPointer<CliStartStream::Launcher> m_StreamLauncher;
    QScopedPointer<CliPair::Launcher> m_PairLauncher;
    QScopedPointer<CliQuitStream::Launcher> m_QuitLauncher;
    FrontendSessionCoordinator m_SessionCoordinator;
    QWidget m_WindowContextSource;
    QScopedPointer<QtWidgetWindowContext> m_WindowContext;
    QPointer<Session> m_ActiveSession;
    QString m_StreamError;
};
