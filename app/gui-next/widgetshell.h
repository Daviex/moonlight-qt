#pragma once

#include "backend/computermanager.h"
#include "frontend/applicationfacade.h"
#include "gui-next/widgetscontrolleradapter.h"
#include "streaming/qtwidgetwindowcontext.h"

#include <QListWidget>
#include <QMainWindow>
#include <QPointer>
#include <QScopedPointer>
#include <QStackedWidget>

class QLabel;
class QKeyEvent;
class QPushButton;
class QSpinBox;
class QCheckBox;

class GuiNextWindow : public QMainWindow
{
    Q_OBJECT

public:
    explicit GuiNextWindow(QWidget* parent = nullptr);
    ~GuiNextWindow() override;

protected:
    void changeEvent(QEvent* event) override;
    void closeEvent(QCloseEvent* event) override;
    void keyPressEvent(QKeyEvent* event) override;
    void showEvent(QShowEvent* event) override;

private slots:
    void refreshHosts();
    void openSelectedHost();
    void pairSelectedHost();
    void wakeSelectedHost();
    void deleteSelectedHost();
    void renameSelectedHost();
    void addHost();
    void showSettings();
    void saveSettings();
    void showHostsPage();
    void refreshApps();
    void launchSelectedApp();
    void quitRunningApp();

private:
    int selectedHostIndex() const;
    int selectedAppIndex() const;
    void buildHostPage();
    void buildAppPage();
    void buildSettingsPage();
    void launchSession(Session* session, const QString& appName, bool isResume);
    void setStatusText(const QString& text);

    QScopedPointer<ComputerManager> m_ComputerManager;
    FrontendApplicationFacade m_Facade;
    WidgetsControllerAdapter m_ControllerAdapter;
    QScopedPointer<AppListFacade> m_AppList;
    QScopedPointer<QtWidgetWindowContext> m_WindowContext;
    QPointer<Session> m_ActiveSession;

    QStackedWidget* m_Stack = nullptr;
    QListWidget* m_HostList = nullptr;
    QListWidget* m_AppListWidget = nullptr;
    QLabel* m_StatusLabel = nullptr;
    QLabel* m_AppHeaderLabel = nullptr;
    QSpinBox* m_WidthSpinBox = nullptr;
    QSpinBox* m_HeightSpinBox = nullptr;
    QSpinBox* m_FpsSpinBox = nullptr;
    QSpinBox* m_BitrateSpinBox = nullptr;
    QCheckBox* m_HdrCheckBox = nullptr;
    QCheckBox* m_Yuv444CheckBox = nullptr;
    int m_CurrentComputerIndex = -1;
};
