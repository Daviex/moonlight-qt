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
class QComboBox;
class QMessageBox;
class QPushButton;
class QSpinBox;
class QCheckBox;

class GuiNextWindow : public QMainWindow
{
    Q_OBJECT

public:
    explicit GuiNextWindow(QWidget* parent = nullptr);
    ~GuiNextWindow() override;

    void showPreferredWindowState();

protected:
    void changeEvent(QEvent* event) override;
    void closeEvent(QCloseEvent* event) override;
    void keyPressEvent(QKeyEvent* event) override;
    void showEvent(QShowEvent* event) override;

private slots:
    void refreshHosts();
    void openSelectedHost();
    void openSelectedHostAllApps();
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
    void toggleSelectedAppHidden();
    void toggleSelectedAppDirectLaunch();
    void runStartupChecks();
    void handlePairingCompleted(const QString& error);
    void handleHardwareAccelerationChanged();
    void handleUnmappedGamepadsChanged();
    void handleUpdateAvailable(const QString& newVersion, const QString& url);

private:
    int selectedHostIndex() const;
    int selectedAppIndex() const;
    void openHost(int index, bool showHiddenGames, bool allowDirectLaunch);
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
    QPointer<QMessageBox> m_PairingDialog;
    QLabel* m_StatusLabel = nullptr;
    QLabel* m_AppHeaderLabel = nullptr;
    QSpinBox* m_WidthSpinBox = nullptr;
    QSpinBox* m_HeightSpinBox = nullptr;
    QSpinBox* m_FpsSpinBox = nullptr;
    QSpinBox* m_BitrateSpinBox = nullptr;
    QSpinBox* m_PacketSizeSpinBox = nullptr;
    QComboBox* m_AudioConfigComboBox = nullptr;
    QComboBox* m_VideoCodecComboBox = nullptr;
    QComboBox* m_VideoDecoderComboBox = nullptr;
    QComboBox* m_WindowModeComboBox = nullptr;
    QComboBox* m_UiModeComboBox = nullptr;
    QComboBox* m_CaptureSysKeysComboBox = nullptr;
    QCheckBox* m_UnlockBitrateCheckBox = nullptr;
    QCheckBox* m_AutoAdjustBitrateCheckBox = nullptr;
    QCheckBox* m_VsyncCheckBox = nullptr;
    QCheckBox* m_GameOptimizationsCheckBox = nullptr;
    QCheckBox* m_HostAudioCheckBox = nullptr;
    QCheckBox* m_MultiControllerCheckBox = nullptr;
    QCheckBox* m_MdnsCheckBox = nullptr;
    QCheckBox* m_QuitAppAfterCheckBox = nullptr;
    QCheckBox* m_AbsoluteMouseCheckBox = nullptr;
    QCheckBox* m_AbsoluteTouchCheckBox = nullptr;
    QCheckBox* m_FramePacingCheckBox = nullptr;
    QCheckBox* m_ConnectionWarningsCheckBox = nullptr;
    QCheckBox* m_ConfigWarningsCheckBox = nullptr;
    QCheckBox* m_RichPresenceCheckBox = nullptr;
    QCheckBox* m_GamepadMouseCheckBox = nullptr;
    QCheckBox* m_DetectNetworkBlockingCheckBox = nullptr;
    QCheckBox* m_PerformanceOverlayCheckBox = nullptr;
    QCheckBox* m_SwapMouseButtonsCheckBox = nullptr;
    QCheckBox* m_MuteOnFocusLossCheckBox = nullptr;
    QCheckBox* m_BackgroundGamepadCheckBox = nullptr;
    QCheckBox* m_ReverseScrollCheckBox = nullptr;
    QCheckBox* m_SwapFaceButtonsCheckBox = nullptr;
    QCheckBox* m_KeepAwakeCheckBox = nullptr;
    QCheckBox* m_HdrCheckBox = nullptr;
    QCheckBox* m_Yuv444CheckBox = nullptr;
    int m_CurrentComputerIndex = -1;
    bool m_PairingInProgress = false;
    bool m_StartupChecksStarted = false;
    bool m_HardwareWarningShown = false;
    bool m_UnmappedGamepadWarningShown = false;
};
