#include "widgetshell.h"

#include "frontend/applistfacade.h"
#include "settings/streamingpreferences.h"

#include <QApplication>
#include <QCheckBox>
#include <QCloseEvent>
#include <QComboBox>
#include <QDesktopServices>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QInputDialog>
#include <QKeyEvent>
#include <QLabel>
#include <QMessageBox>
#include <QPushButton>
#include <QScrollArea>
#include <QSpinBox>
#include <QTimer>
#include <QUrl>
#include <QVBoxLayout>

namespace {
    constexpr int HostPageIndex = 0;
    constexpr int AppPageIndex = 1;
    constexpr int SettingsPageIndex = 2;

    void addComboItem(QComboBox* comboBox, const QString& label, int value)
    {
        comboBox->addItem(label, value);
    }

    void setComboValue(QComboBox* comboBox, int value)
    {
        int index = comboBox->findData(value);
        if (index >= 0) {
            comboBox->setCurrentIndex(index);
        }
    }

    int comboValue(QComboBox* comboBox)
    {
        return comboBox->currentData().toInt();
    }
}

GuiNextWindow::GuiNextWindow(QWidget* parent)
    : QMainWindow(parent),
      m_ComputerManager(new ComputerManager(StreamingPreferences::get())),
      m_Facade(this),
      m_ControllerAdapter(StreamingPreferences::get(), this)
{
    setWindowTitle(tr("Moonlight"));
    resize(960, 540);

    m_Facade.initialize(m_ComputerManager.data());

    m_Stack = new QStackedWidget(this);
    setCentralWidget(m_Stack);
    buildHostPage();
    buildAppPage();
    buildSettingsPage();

    connect(m_Facade.computers(), &ComputerListFacade::computersReset,
            this, &GuiNextWindow::refreshHosts);
    connect(m_Facade.computers(), &ComputerListFacade::computerChanged,
            this, &GuiNextWindow::refreshHosts);
    connect(m_Facade.computers(), &ComputerListFacade::pairingCompleted,
            this, [this](const QString& error) {
        if (error.isEmpty()) {
            setStatusText(tr("Pairing completed."));
        }
        else {
            QMessageBox::warning(this, tr("Pairing Failed"), error);
        }
        refreshHosts();
    });

    connect(m_Facade.sessions(), &FrontendSessionCoordinator::hideUiRequested,
            this, &QWidget::hide);
    connect(m_Facade.sessions(), &FrontendSessionCoordinator::showUiRequested,
            this, &QWidget::show);
    connect(m_Facade.sessions(), &FrontendSessionCoordinator::errorTextChanged,
            this, [this](const QString& error) {
        if (!error.isEmpty()) {
            QMessageBox::warning(this, tr("Stream Error"), error);
        }
    });
    connect(m_Facade.sessions(), &FrontendSessionCoordinator::launchWarningsChanged,
            this, [this](const QStringList& warnings) {
        for (const QString& warning : warnings) {
            QMessageBox::information(this, tr("Launch Warning"), warning);
        }
    });
    connect(m_Facade.sessions(), &FrontendSessionCoordinator::quitApplicationRequested,
            qApp, &QCoreApplication::quit);
    connect(m_Facade.system(), &SystemFacade::hasHardwareAccelerationChanged,
            this, &GuiNextWindow::handleHardwareAccelerationChanged);
    connect(m_Facade.system(), &SystemFacade::unmappedGamepadsChanged,
            this, &GuiNextWindow::handleUnmappedGamepadsChanged);
    connect(m_Facade.updates(), &UpdateFacade::updateAvailable,
            this, &GuiNextWindow::handleUpdateAvailable);

    m_ComputerManager->startPolling();
    refreshHosts();
}

GuiNextWindow::~GuiNextWindow()
{
    m_ControllerAdapter.disable();
    m_ComputerManager->stopPollingAsync();
}

void GuiNextWindow::buildHostPage()
{
    auto page = new QWidget(this);
    auto layout = new QVBoxLayout(page);
    auto title = new QLabel(tr("Moonlight"), page);
    title->setStyleSheet(QStringLiteral("font-size: 28px; font-weight: bold;"));
    layout->addWidget(title);

    m_HostList = new QListWidget(page);
    m_HostList->setAlternatingRowColors(true);
    connect(m_HostList, &QListWidget::itemActivated,
            this, &GuiNextWindow::openSelectedHost);
    connect(m_HostList, &QListWidget::itemDoubleClicked,
            this, &GuiNextWindow::openSelectedHost);
    layout->addWidget(m_HostList, 1);

    auto buttons = new QHBoxLayout();
    auto refreshButton = new QPushButton(tr("Refresh"), page);
    auto addButton = new QPushButton(tr("Add Host"), page);
    auto appsButton = new QPushButton(tr("Apps / Resume"), page);
    auto pairButton = new QPushButton(tr("Pair"), page);
    auto wakeButton = new QPushButton(tr("Wake"), page);
    auto renameButton = new QPushButton(tr("Rename"), page);
    auto deleteButton = new QPushButton(tr("Delete"), page);
    auto settingsButton = new QPushButton(tr("Settings"), page);
    buttons->addWidget(refreshButton);
    buttons->addWidget(addButton);
    buttons->addWidget(appsButton);
    buttons->addWidget(pairButton);
    buttons->addWidget(wakeButton);
    buttons->addWidget(renameButton);
    buttons->addWidget(deleteButton);
    buttons->addStretch();
    buttons->addWidget(settingsButton);
    layout->addLayout(buttons);

    m_StatusLabel = new QLabel(page);
    layout->addWidget(m_StatusLabel);

    connect(refreshButton, &QPushButton::clicked, this, &GuiNextWindow::refreshHosts);
    connect(addButton, &QPushButton::clicked, this, &GuiNextWindow::addHost);
    connect(appsButton, &QPushButton::clicked, this, &GuiNextWindow::openSelectedHost);
    connect(pairButton, &QPushButton::clicked, this, &GuiNextWindow::pairSelectedHost);
    connect(wakeButton, &QPushButton::clicked, this, &GuiNextWindow::wakeSelectedHost);
    connect(renameButton, &QPushButton::clicked, this, &GuiNextWindow::renameSelectedHost);
    connect(deleteButton, &QPushButton::clicked, this, &GuiNextWindow::deleteSelectedHost);
    connect(settingsButton, &QPushButton::clicked, this, &GuiNextWindow::showSettings);

    m_Stack->addWidget(page);
}

void GuiNextWindow::buildAppPage()
{
    auto page = new QWidget(this);
    auto layout = new QVBoxLayout(page);
    m_AppHeaderLabel = new QLabel(page);
    m_AppHeaderLabel->setStyleSheet(QStringLiteral("font-size: 24px; font-weight: bold;"));
    layout->addWidget(m_AppHeaderLabel);

    m_AppListWidget = new QListWidget(page);
    connect(m_AppListWidget, &QListWidget::itemActivated,
            this, &GuiNextWindow::launchSelectedApp);
    connect(m_AppListWidget, &QListWidget::itemDoubleClicked,
            this, &GuiNextWindow::launchSelectedApp);
    layout->addWidget(m_AppListWidget, 1);

    auto buttons = new QHBoxLayout();
    auto backButton = new QPushButton(tr("Back"), page);
    auto launchButton = new QPushButton(tr("Launch"), page);
    auto quitButton = new QPushButton(tr("Quit Running App"), page);
    buttons->addWidget(backButton);
    buttons->addStretch();
    buttons->addWidget(quitButton);
    buttons->addWidget(launchButton);
    layout->addLayout(buttons);

    connect(backButton, &QPushButton::clicked, this, &GuiNextWindow::showHostsPage);
    connect(launchButton, &QPushButton::clicked, this, &GuiNextWindow::launchSelectedApp);
    connect(quitButton, &QPushButton::clicked, this, &GuiNextWindow::quitRunningApp);

    m_Stack->addWidget(page);
}

void GuiNextWindow::showPreferredWindowState()
{
    const FrontendSystemProperties system = m_Facade.system()->properties();
    if (!system.hasDesktopEnvironment) {
        showFullScreen();
        return;
    }

    const FrontendStreamingPreferences preferences = m_Facade.preferences()->preferences();
    switch (preferences.uiDisplayMode) {
    case StreamingPreferences::UI_MAXIMIZED:
        showMaximized();
        break;
    case StreamingPreferences::UI_FULLSCREEN:
        showFullScreen();
        break;
    case StreamingPreferences::UI_WINDOWED:
    default:
        show();
        break;
    }
}

void GuiNextWindow::buildSettingsPage()
{
    auto page = new QWidget(this);
    auto layout = new QVBoxLayout(page);
    auto title = new QLabel(tr("Settings"), page);
    title->setStyleSheet(QStringLiteral("font-size: 24px; font-weight: bold;"));
    layout->addWidget(title);

    auto scrollArea = new QScrollArea(page);
    scrollArea->setWidgetResizable(true);
    auto settingsContent = new QWidget(scrollArea);
    auto form = new QFormLayout(settingsContent);
    m_WidthSpinBox = new QSpinBox(page);
    m_WidthSpinBox->setRange(1, 16384);
    m_HeightSpinBox = new QSpinBox(page);
    m_HeightSpinBox->setRange(1, 16384);
    m_FpsSpinBox = new QSpinBox(page);
    m_FpsSpinBox->setRange(10, 480);
    m_BitrateSpinBox = new QSpinBox(page);
    m_BitrateSpinBox->setRange(500, 500000);
    m_BitrateSpinBox->setSuffix(tr(" Kbps"));
    m_PacketSizeSpinBox = new QSpinBox(page);
    m_PacketSizeSpinBox->setRange(0, 9000);
    m_PacketSizeSpinBox->setSpecialValueText(tr("Automatic"));
    m_AudioConfigComboBox = new QComboBox(page);
    addComboItem(m_AudioConfigComboBox, tr("Stereo"), StreamingPreferences::AC_STEREO);
    addComboItem(m_AudioConfigComboBox, tr("5.1 surround"), StreamingPreferences::AC_51_SURROUND);
    addComboItem(m_AudioConfigComboBox, tr("7.1 surround"), StreamingPreferences::AC_71_SURROUND);
    m_VideoCodecComboBox = new QComboBox(page);
    addComboItem(m_VideoCodecComboBox, tr("Automatic"), StreamingPreferences::VCC_AUTO);
    addComboItem(m_VideoCodecComboBox, tr("Force H.264"), StreamingPreferences::VCC_FORCE_H264);
    addComboItem(m_VideoCodecComboBox, tr("Force HEVC"), StreamingPreferences::VCC_FORCE_HEVC);
    addComboItem(m_VideoCodecComboBox, tr("Force AV1"), StreamingPreferences::VCC_FORCE_AV1);
    m_VideoDecoderComboBox = new QComboBox(page);
    addComboItem(m_VideoDecoderComboBox, tr("Automatic"), StreamingPreferences::VDS_AUTO);
    addComboItem(m_VideoDecoderComboBox, tr("Force hardware"), StreamingPreferences::VDS_FORCE_HARDWARE);
    addComboItem(m_VideoDecoderComboBox, tr("Force software"), StreamingPreferences::VDS_FORCE_SOFTWARE);
    m_WindowModeComboBox = new QComboBox(page);
    addComboItem(m_WindowModeComboBox, tr("Fullscreen"), StreamingPreferences::WM_FULLSCREEN);
    addComboItem(m_WindowModeComboBox, tr("Borderless fullscreen"), StreamingPreferences::WM_FULLSCREEN_DESKTOP);
    addComboItem(m_WindowModeComboBox, tr("Windowed"), StreamingPreferences::WM_WINDOWED);
    m_UiModeComboBox = new QComboBox(page);
    addComboItem(m_UiModeComboBox, tr("Windowed"), StreamingPreferences::UI_WINDOWED);
    addComboItem(m_UiModeComboBox, tr("Maximized"), StreamingPreferences::UI_MAXIMIZED);
    addComboItem(m_UiModeComboBox, tr("Fullscreen"), StreamingPreferences::UI_FULLSCREEN);
    m_CaptureSysKeysComboBox = new QComboBox(page);
    addComboItem(m_CaptureSysKeysComboBox, tr("Off"), StreamingPreferences::CSK_OFF);
    addComboItem(m_CaptureSysKeysComboBox, tr("Fullscreen only"), StreamingPreferences::CSK_FULLSCREEN);
    addComboItem(m_CaptureSysKeysComboBox, tr("Always"), StreamingPreferences::CSK_ALWAYS);
    m_UnlockBitrateCheckBox = new QCheckBox(page);
    m_AutoAdjustBitrateCheckBox = new QCheckBox(page);
    m_VsyncCheckBox = new QCheckBox(page);
    m_GameOptimizationsCheckBox = new QCheckBox(page);
    m_HostAudioCheckBox = new QCheckBox(page);
    m_MultiControllerCheckBox = new QCheckBox(page);
    m_MdnsCheckBox = new QCheckBox(page);
    m_QuitAppAfterCheckBox = new QCheckBox(page);
    m_AbsoluteMouseCheckBox = new QCheckBox(page);
    m_AbsoluteTouchCheckBox = new QCheckBox(page);
    m_FramePacingCheckBox = new QCheckBox(page);
    m_ConnectionWarningsCheckBox = new QCheckBox(page);
    m_ConfigWarningsCheckBox = new QCheckBox(page);
    m_RichPresenceCheckBox = new QCheckBox(page);
    m_GamepadMouseCheckBox = new QCheckBox(page);
    m_DetectNetworkBlockingCheckBox = new QCheckBox(page);
    m_PerformanceOverlayCheckBox = new QCheckBox(page);
    m_SwapMouseButtonsCheckBox = new QCheckBox(page);
    m_MuteOnFocusLossCheckBox = new QCheckBox(page);
    m_BackgroundGamepadCheckBox = new QCheckBox(page);
    m_ReverseScrollCheckBox = new QCheckBox(page);
    m_SwapFaceButtonsCheckBox = new QCheckBox(page);
    m_KeepAwakeCheckBox = new QCheckBox(page);
    m_HdrCheckBox = new QCheckBox(page);
    m_Yuv444CheckBox = new QCheckBox(page);
    form->addRow(tr("Width"), m_WidthSpinBox);
    form->addRow(tr("Height"), m_HeightSpinBox);
    form->addRow(tr("FPS"), m_FpsSpinBox);
    form->addRow(tr("Bitrate"), m_BitrateSpinBox);
    form->addRow(tr("Packet size"), m_PacketSizeSpinBox);
    form->addRow(tr("Audio"), m_AudioConfigComboBox);
    form->addRow(tr("Video codec"), m_VideoCodecComboBox);
    form->addRow(tr("Video decoder"), m_VideoDecoderComboBox);
    form->addRow(tr("Stream window mode"), m_WindowModeComboBox);
    form->addRow(tr("UI startup mode"), m_UiModeComboBox);
    form->addRow(tr("Capture system keys"), m_CaptureSysKeysComboBox);
    form->addRow(tr("Unlock bitrate limit"), m_UnlockBitrateCheckBox);
    form->addRow(tr("Automatically adjust bitrate"), m_AutoAdjustBitrateCheckBox);
    form->addRow(tr("V-Sync"), m_VsyncCheckBox);
    form->addRow(tr("Optimize game settings"), m_GameOptimizationsCheckBox);
    form->addRow(tr("Play audio on host"), m_HostAudioCheckBox);
    form->addRow(tr("Multiple controllers"), m_MultiControllerCheckBox);
    form->addRow(tr("mDNS discovery"), m_MdnsCheckBox);
    form->addRow(tr("Quit app after stream"), m_QuitAppAfterCheckBox);
    form->addRow(tr("Absolute mouse mode"), m_AbsoluteMouseCheckBox);
    form->addRow(tr("Absolute touch mode"), m_AbsoluteTouchCheckBox);
    form->addRow(tr("Frame pacing"), m_FramePacingCheckBox);
    form->addRow(tr("Connection warnings"), m_ConnectionWarningsCheckBox);
    form->addRow(tr("Configuration warnings"), m_ConfigWarningsCheckBox);
    form->addRow(tr("Discord rich presence"), m_RichPresenceCheckBox);
    form->addRow(tr("Gamepad mouse"), m_GamepadMouseCheckBox);
    form->addRow(tr("Detect network blocking"), m_DetectNetworkBlockingCheckBox);
    form->addRow(tr("Performance overlay"), m_PerformanceOverlayCheckBox);
    form->addRow(tr("Swap mouse buttons"), m_SwapMouseButtonsCheckBox);
    form->addRow(tr("Mute on focus loss"), m_MuteOnFocusLossCheckBox);
    form->addRow(tr("Background gamepad input"), m_BackgroundGamepadCheckBox);
    form->addRow(tr("Reverse scroll direction"), m_ReverseScrollCheckBox);
    form->addRow(tr("Swap controller face buttons"), m_SwapFaceButtonsCheckBox);
    form->addRow(tr("Keep display awake"), m_KeepAwakeCheckBox);
    form->addRow(tr("HDR"), m_HdrCheckBox);
    form->addRow(tr("YUV 4:4:4"), m_Yuv444CheckBox);
    scrollArea->setWidget(settingsContent);
    layout->addWidget(scrollArea, 1);

    auto buttons = new QHBoxLayout();
    auto backButton = new QPushButton(tr("Back"), page);
    auto saveButton = new QPushButton(tr("Save"), page);
    buttons->addWidget(backButton);
    buttons->addStretch();
    buttons->addWidget(saveButton);
    layout->addLayout(buttons);

    connect(backButton, &QPushButton::clicked, this, &GuiNextWindow::showHostsPage);
    connect(saveButton, &QPushButton::clicked, this, &GuiNextWindow::saveSettings);

    m_Stack->addWidget(page);
}

void GuiNextWindow::refreshHosts()
{
    int selected = selectedHostIndex();
    m_HostList->clear();

    QVector<FrontendComputer> computers = m_Facade.computers()->computers();
    for (int i = 0; i < computers.count(); i++) {
        const FrontendComputer& computer = computers.at(i);
        QString status = computer.statusUnknown ? tr("Unknown") : (computer.online ? tr("Online") : tr("Offline"));
        QString pairState = computer.paired ? tr("Paired") : tr("Unpaired");
        auto item = new QListWidgetItem(QStringLiteral("%1\n%2 - %3").arg(computer.name, status, pairState));
        item->setData(Qt::UserRole, i);
        m_HostList->addItem(item);
        if (i == selected) {
            m_HostList->setCurrentItem(item);
        }
    }

    if (m_HostList->currentItem() == nullptr && m_HostList->count() > 0) {
        m_HostList->setCurrentRow(0);
    }

    setStatusText(tr("%1 host(s)").arg(computers.count()));
}

void GuiNextWindow::openSelectedHost()
{
    int index = selectedHostIndex();
    if (index < 0) {
        return;
    }

    FrontendComputer computer = m_Facade.computers()->computerAt(index);
    if (computer.busy) {
        Session* session = m_Facade.computers()->createSessionForCurrentGame(index);
        launchSession(session, computer.name, true);
        return;
    }
    if (!computer.online) {
        QMessageBox::information(this, tr("Host Offline"), tr("This host is not online."));
        return;
    }
    if (!computer.paired) {
        pairSelectedHost();
        return;
    }

    m_CurrentComputerIndex = index;
    m_AppList.reset(m_Facade.createAppList(index, false, this));
    if (m_AppList == nullptr) {
        return;
    }

    connect(m_AppList.data(), &AppListFacade::appsReset, this, &GuiNextWindow::refreshApps);
    connect(m_AppList.data(), &AppListFacade::appChanged, this, &GuiNextWindow::refreshApps);
    connect(m_AppList.data(), &AppListFacade::computerLost, this, [this]() {
        QMessageBox::warning(this, tr("Host Lost"), tr("The selected host is no longer available."));
        showHostsPage();
    });

    m_AppHeaderLabel->setText(computer.name);
    refreshApps();
    m_ControllerAdapter.setUiNavMode(false);
    m_Stack->setCurrentIndex(AppPageIndex);
}

void GuiNextWindow::refreshApps()
{
    if (m_AppList == nullptr) {
        return;
    }

    int selected = selectedAppIndex();
    m_AppListWidget->clear();
    QVector<FrontendApp> apps = m_AppList->apps();
    for (int i = 0; i < apps.count(); i++) {
        const FrontendApp& app = apps.at(i);
        QString suffix = app.running ? tr(" (running)") : QString();
        auto item = new QListWidgetItem(app.name + suffix);
        item->setData(Qt::UserRole, i);
        m_AppListWidget->addItem(item);
        if (i == selected) {
            m_AppListWidget->setCurrentItem(item);
        }
    }

    if (m_AppListWidget->currentItem() == nullptr && m_AppListWidget->count() > 0) {
        m_AppListWidget->setCurrentRow(0);
    }
}

void GuiNextWindow::launchSelectedApp()
{
    if (m_AppList == nullptr) {
        return;
    }

    int appIndex = selectedAppIndex();
    if (appIndex < 0) {
        return;
    }

    FrontendApp app = m_AppList->appAt(appIndex);
    Session* session = m_AppList->createSessionForApp(appIndex);
    launchSession(session, app.name, false);
}

void GuiNextWindow::launchSession(Session* session, const QString& appName, bool isResume)
{
    if (session == nullptr) {
        QMessageBox::warning(this, tr("Stream Error"), tr("Unable to start stream: session was not created."));
        return;
    }

    m_ActiveSession = session;
    connect(session, &Session::readyForDeletion, session, &QObject::deleteLater);

    m_WindowContext.reset(new QtWidgetWindowContext(this));
    m_Facade.system()->waitForAsyncLoad();
    m_Facade.sessions()->setSession(session, appName, isResume, false);

    m_ControllerAdapter.disable();
    if (!m_Facade.sessions()->initialize(m_WindowContext.data())) {
        m_Facade.sessions()->clearSession();
        session->deleteLater();
        m_ControllerAdapter.enable();
        return;
    }

    m_Facade.sessions()->start();
    m_ControllerAdapter.enable();
}

void GuiNextWindow::pairSelectedHost()
{
    int index = selectedHostIndex();
    if (index < 0) {
        return;
    }

    QString pin = m_Facade.computers()->generatePinString();
    QMessageBox::information(this, tr("Pair Host"),
                             tr("Enter this PIN on your host PC when prompted: %1").arg(pin));
    m_Facade.computers()->pairComputer(index, pin);
}

void GuiNextWindow::wakeSelectedHost()
{
    int index = selectedHostIndex();
    if (index >= 0) {
        m_Facade.computers()->wakeComputer(index);
    }
}

void GuiNextWindow::deleteSelectedHost()
{
    int index = selectedHostIndex();
    if (index < 0) {
        return;
    }

    if (QMessageBox::question(this, tr("Delete Host"), tr("Delete this host?")) == QMessageBox::Yes) {
        m_Facade.computers()->deleteComputer(index);
    }
}

void GuiNextWindow::renameSelectedHost()
{
    int index = selectedHostIndex();
    if (index < 0) {
        return;
    }

    bool ok = false;
    QString name = QInputDialog::getText(this, tr("Rename Host"), tr("Name:"), QLineEdit::Normal,
                                         m_Facade.computers()->computerAt(index).name, &ok);
    if (ok && !name.trimmed().isEmpty()) {
        m_Facade.computers()->renameComputer(index, name.trimmed());
    }
}

void GuiNextWindow::addHost()
{
    bool ok = false;
    QString address = QInputDialog::getText(this, tr("Add Host"), tr("Hostname or IP address:"), QLineEdit::Normal,
                                            QString(), &ok);
    if (ok && !address.trimmed().isEmpty()) {
        m_ComputerManager->addNewHostManually(address.trimmed());
    }
}

void GuiNextWindow::showSettings()
{
    FrontendStreamingPreferences preferences = m_Facade.preferences()->preferences();
    m_WidthSpinBox->setValue(preferences.width);
    m_HeightSpinBox->setValue(preferences.height);
    m_FpsSpinBox->setValue(preferences.fps);
    m_BitrateSpinBox->setValue(preferences.bitrateKbps);
    m_PacketSizeSpinBox->setValue(preferences.packetSize);
    setComboValue(m_AudioConfigComboBox, preferences.audioConfig);
    setComboValue(m_VideoCodecComboBox, preferences.videoCodecConfig);
    setComboValue(m_VideoDecoderComboBox, preferences.videoDecoderSelection);
    setComboValue(m_WindowModeComboBox, preferences.windowMode);
    setComboValue(m_UiModeComboBox, preferences.uiDisplayMode);
    setComboValue(m_CaptureSysKeysComboBox, preferences.captureSysKeysMode);
    m_UnlockBitrateCheckBox->setChecked(preferences.unlockBitrate);
    m_AutoAdjustBitrateCheckBox->setChecked(preferences.autoAdjustBitrate);
    m_VsyncCheckBox->setChecked(preferences.enableVsync);
    m_GameOptimizationsCheckBox->setChecked(preferences.gameOptimizations);
    m_HostAudioCheckBox->setChecked(preferences.playAudioOnHost);
    m_MultiControllerCheckBox->setChecked(preferences.multiController);
    m_MdnsCheckBox->setChecked(preferences.enableMdns);
    m_QuitAppAfterCheckBox->setChecked(preferences.quitAppAfter);
    m_AbsoluteMouseCheckBox->setChecked(preferences.absoluteMouseMode);
    m_AbsoluteTouchCheckBox->setChecked(preferences.absoluteTouchMode);
    m_FramePacingCheckBox->setChecked(preferences.framePacing);
    m_ConnectionWarningsCheckBox->setChecked(preferences.connectionWarnings);
    m_ConfigWarningsCheckBox->setChecked(preferences.configurationWarnings);
    m_RichPresenceCheckBox->setChecked(preferences.richPresence);
    m_GamepadMouseCheckBox->setChecked(preferences.gamepadMouse);
    m_DetectNetworkBlockingCheckBox->setChecked(preferences.detectNetworkBlocking);
    m_PerformanceOverlayCheckBox->setChecked(preferences.showPerformanceOverlay);
    m_SwapMouseButtonsCheckBox->setChecked(preferences.swapMouseButtons);
    m_MuteOnFocusLossCheckBox->setChecked(preferences.muteOnFocusLoss);
    m_BackgroundGamepadCheckBox->setChecked(preferences.backgroundGamepad);
    m_ReverseScrollCheckBox->setChecked(preferences.reverseScrollDirection);
    m_SwapFaceButtonsCheckBox->setChecked(preferences.swapFaceButtons);
    m_KeepAwakeCheckBox->setChecked(preferences.keepAwake);
    m_HdrCheckBox->setChecked(preferences.enableHdr);
    m_Yuv444CheckBox->setChecked(preferences.enableYUV444);
    m_ControllerAdapter.setUiNavMode(true);
    m_Stack->setCurrentIndex(SettingsPageIndex);
}

void GuiNextWindow::saveSettings()
{
    FrontendStreamingPreferences preferences = m_Facade.preferences()->preferences();
    preferences.width = m_WidthSpinBox->value();
    preferences.height = m_HeightSpinBox->value();
    preferences.fps = m_FpsSpinBox->value();
    preferences.bitrateKbps = m_BitrateSpinBox->value();
    preferences.packetSize = m_PacketSizeSpinBox->value();
    preferences.audioConfig = comboValue(m_AudioConfigComboBox);
    preferences.videoCodecConfig = comboValue(m_VideoCodecComboBox);
    preferences.videoDecoderSelection = comboValue(m_VideoDecoderComboBox);
    preferences.windowMode = comboValue(m_WindowModeComboBox);
    preferences.uiDisplayMode = comboValue(m_UiModeComboBox);
    preferences.captureSysKeysMode = comboValue(m_CaptureSysKeysComboBox);
    preferences.unlockBitrate = m_UnlockBitrateCheckBox->isChecked();
    preferences.autoAdjustBitrate = m_AutoAdjustBitrateCheckBox->isChecked();
    preferences.enableVsync = m_VsyncCheckBox->isChecked();
    preferences.gameOptimizations = m_GameOptimizationsCheckBox->isChecked();
    preferences.playAudioOnHost = m_HostAudioCheckBox->isChecked();
    preferences.multiController = m_MultiControllerCheckBox->isChecked();
    preferences.enableMdns = m_MdnsCheckBox->isChecked();
    preferences.quitAppAfter = m_QuitAppAfterCheckBox->isChecked();
    preferences.absoluteMouseMode = m_AbsoluteMouseCheckBox->isChecked();
    preferences.absoluteTouchMode = m_AbsoluteTouchCheckBox->isChecked();
    preferences.framePacing = m_FramePacingCheckBox->isChecked();
    preferences.connectionWarnings = m_ConnectionWarningsCheckBox->isChecked();
    preferences.configurationWarnings = m_ConfigWarningsCheckBox->isChecked();
    preferences.richPresence = m_RichPresenceCheckBox->isChecked();
    preferences.gamepadMouse = m_GamepadMouseCheckBox->isChecked();
    preferences.detectNetworkBlocking = m_DetectNetworkBlockingCheckBox->isChecked();
    preferences.showPerformanceOverlay = m_PerformanceOverlayCheckBox->isChecked();
    preferences.swapMouseButtons = m_SwapMouseButtonsCheckBox->isChecked();
    preferences.muteOnFocusLoss = m_MuteOnFocusLossCheckBox->isChecked();
    preferences.backgroundGamepad = m_BackgroundGamepadCheckBox->isChecked();
    preferences.reverseScrollDirection = m_ReverseScrollCheckBox->isChecked();
    preferences.swapFaceButtons = m_SwapFaceButtonsCheckBox->isChecked();
    preferences.keepAwake = m_KeepAwakeCheckBox->isChecked();
    preferences.enableHdr = m_HdrCheckBox->isChecked();
    preferences.enableYUV444 = m_Yuv444CheckBox->isChecked();
    m_Facade.preferences()->applyPreferences(preferences, true);
    showHostsPage();
}

void GuiNextWindow::showHostsPage()
{
    m_ControllerAdapter.setUiNavMode(false);
    m_Stack->setCurrentIndex(HostPageIndex);
    refreshHosts();
}

void GuiNextWindow::quitRunningApp()
{
    if (m_AppList != nullptr) {
        m_AppList->quitRunningApp();
    }
}

void GuiNextWindow::runStartupChecks()
{
    const FrontendSystemProperties system = m_Facade.system()->properties();
    if (system.isWow64) {
        const QMessageBox::StandardButton result = QMessageBox::question(
            this,
            tr("Moonlight"),
            tr("This version of Moonlight isn't optimized for your PC. Please download the '%1' version of Moonlight for the best streaming performance.").arg(system.friendlyNativeArchName),
            QMessageBox::Ok | QMessageBox::Cancel);
        if (result == QMessageBox::Ok) {
            QDesktopServices::openUrl(QUrl(QStringLiteral("https://github.com/moonlight-stream/moonlight-qt/releases")));
        }
    }

    m_Facade.system()->startAsyncLoad();
    m_Facade.updates()->start();
}

void GuiNextWindow::handleHardwareAccelerationChanged()
{
    if (m_HardwareWarningShown) {
        return;
    }

    const FrontendSystemProperties system = m_Facade.system()->properties();
    const FrontendStreamingPreferences preferences = m_Facade.preferences()->preferences();
    if (system.hasHardwareAcceleration ||
            preferences.videoDecoderSelection == StreamingPreferences::VDS_FORCE_SOFTWARE) {
        return;
    }

    m_HardwareWarningShown = true;
    if (system.isRunningXWayland) {
        QMessageBox::warning(this,
                             tr("Hardware Acceleration"),
                             tr("Hardware acceleration doesn't work on XWayland. Continuing on XWayland may result in poor streaming performance. Try running with QT_QPA_PLATFORM=wayland or switch to X11."));
    }
    else {
        QMessageBox::warning(this,
                             tr("Hardware Acceleration"),
                             tr("No functioning hardware accelerated video decoder was detected by Moonlight. Your streaming performance may be severely degraded in this configuration."));
    }
}

void GuiNextWindow::handleUnmappedGamepadsChanged()
{
    if (m_UnmappedGamepadWarningShown) {
        return;
    }

    const QString unmappedGamepads = m_Facade.system()->properties().unmappedGamepads;
    if (unmappedGamepads.isEmpty()) {
        return;
    }

    m_UnmappedGamepadWarningShown = true;
    QMessageBox::warning(this,
                         tr("Gamepad Mapping"),
                         tr("Moonlight detected gamepads without a mapping:") + QLatin1String("\n") + unmappedGamepads);
}

void GuiNextWindow::handleUpdateAvailable(const QString& newVersion, const QString& url)
{
    const FrontendSystemProperties system = m_Facade.system()->properties();
    if (system.hasBrowser) {
        const QMessageBox::StandardButton result = QMessageBox::question(
            this,
            tr("Update Available"),
            tr("Update available for Moonlight: Version %1").arg(newVersion),
            QMessageBox::Open | QMessageBox::Cancel);
        if (result == QMessageBox::Open) {
            QDesktopServices::openUrl(QUrl(url));
        }
    }
    else {
        QMessageBox::information(this,
                                 tr("Update Available"),
                                 tr("Update available for Moonlight: Version %1").arg(newVersion));
    }
}

int GuiNextWindow::selectedHostIndex() const
{
    QListWidgetItem* item = m_HostList != nullptr ? m_HostList->currentItem() : nullptr;
    return item != nullptr ? item->data(Qt::UserRole).toInt() : -1;
}

int GuiNextWindow::selectedAppIndex() const
{
    QListWidgetItem* item = m_AppListWidget != nullptr ? m_AppListWidget->currentItem() : nullptr;
    return item != nullptr ? item->data(Qt::UserRole).toInt() : -1;
}

void GuiNextWindow::changeEvent(QEvent* event)
{
    QMainWindow::changeEvent(event);
    if (event->type() == QEvent::ActivationChange) {
        m_ControllerAdapter.notifyWindowFocus(isActiveWindow());
    }
}

void GuiNextWindow::closeEvent(QCloseEvent* event)
{
    if (m_ActiveSession != nullptr) {
        m_ActiveSession->interrupt();
    }
    QMainWindow::closeEvent(event);
}

void GuiNextWindow::keyPressEvent(QKeyEvent* event)
{
    switch (event->key()) {
    case Qt::Key_Hangup:
        showSettings();
        event->accept();
        return;
    case Qt::Key_Escape:
        if (m_Stack->currentIndex() != HostPageIndex) {
            showHostsPage();
            event->accept();
            return;
        }
        break;
    default:
        break;
    }

    QMainWindow::keyPressEvent(event);
}

void GuiNextWindow::showEvent(QShowEvent* event)
{
    QMainWindow::showEvent(event);
    m_ControllerAdapter.enable();
    m_ControllerAdapter.notifyWindowFocus(isActiveWindow());
    if (!m_StartupChecksStarted) {
        m_StartupChecksStarted = true;
        QTimer::singleShot(0, this, &GuiNextWindow::runStartupChecks);
    }
}

void GuiNextWindow::setStatusText(const QString& text)
{
    if (m_StatusLabel != nullptr) {
        m_StatusLabel->setText(text);
    }
}
