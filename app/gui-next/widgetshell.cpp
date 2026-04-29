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
#include <QIcon>
#include <QInputDialog>
#include <QKeyEvent>
#include <QLabel>
#include <QMessageBox>
#include <QPushButton>
#include <QScrollArea>
#include <QSize>
#include <QSignalBlocker>
#include <QSpinBox>
#include <QStringList>
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

    QString iconPathForUrl(const QUrl& url)
    {
        if (url.isLocalFile()) {
            return url.toLocalFile();
        }

        if (url.scheme() == QStringLiteral("qrc")) {
            return QStringLiteral(":") + url.path();
        }

        return url.toString();
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
            this, &GuiNextWindow::handlePairingCompleted);
    connect(m_Facade.computers(), &ComputerListFacade::connectionTestCompleted,
            this, &GuiNextWindow::handleConnectionTestCompleted);

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
    auto allAppsButton = new QPushButton(tr("View All Apps"), page);
    auto pairButton = new QPushButton(tr("Pair"), page);
    auto wakeButton = new QPushButton(tr("Wake"), page);
    auto testButton = new QPushButton(tr("Test Network"), page);
    auto detailsButton = new QPushButton(tr("Details"), page);
    auto renameButton = new QPushButton(tr("Rename"), page);
    auto deleteButton = new QPushButton(tr("Delete"), page);
    auto helpButton = new QPushButton(tr("Help"), page);
    auto discordButton = new QPushButton(tr("Discord"), page);
    auto settingsButton = new QPushButton(tr("Settings"), page);
    buttons->addWidget(refreshButton);
    buttons->addWidget(addButton);
    buttons->addWidget(appsButton);
    buttons->addWidget(allAppsButton);
    buttons->addWidget(pairButton);
    buttons->addWidget(wakeButton);
    buttons->addWidget(testButton);
    buttons->addWidget(detailsButton);
    buttons->addWidget(renameButton);
    buttons->addWidget(deleteButton);
    buttons->addStretch();
    buttons->addWidget(discordButton);
    buttons->addWidget(helpButton);
    buttons->addWidget(settingsButton);
    layout->addLayout(buttons);

    m_StatusLabel = new QLabel(page);
    layout->addWidget(m_StatusLabel);

    connect(refreshButton, &QPushButton::clicked, this, &GuiNextWindow::refreshHosts);
    connect(addButton, &QPushButton::clicked, this, &GuiNextWindow::addHost);
    connect(appsButton, &QPushButton::clicked, this, &GuiNextWindow::openSelectedHost);
    connect(allAppsButton, &QPushButton::clicked, this, &GuiNextWindow::openSelectedHostAllApps);
    connect(pairButton, &QPushButton::clicked, this, &GuiNextWindow::pairSelectedHost);
    connect(wakeButton, &QPushButton::clicked, this, &GuiNextWindow::wakeSelectedHost);
    connect(testButton, &QPushButton::clicked, this, &GuiNextWindow::testSelectedHostConnection);
    connect(detailsButton, &QPushButton::clicked, this, &GuiNextWindow::showSelectedHostDetails);
    connect(renameButton, &QPushButton::clicked, this, &GuiNextWindow::renameSelectedHost);
    connect(deleteButton, &QPushButton::clicked, this, &GuiNextWindow::deleteSelectedHost);
    connect(helpButton, &QPushButton::clicked, this, &GuiNextWindow::openHelp);
    connect(discordButton, &QPushButton::clicked, this, &GuiNextWindow::openDiscord);
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
    m_AppListWidget->setIconSize(QSize(75, 100));
    connect(m_AppListWidget, &QListWidget::itemActivated,
            this, &GuiNextWindow::launchSelectedApp);
    connect(m_AppListWidget, &QListWidget::itemDoubleClicked,
            this, &GuiNextWindow::launchSelectedApp);
    layout->addWidget(m_AppListWidget, 1);

    auto buttons = new QHBoxLayout();
    auto backButton = new QPushButton(tr("Back"), page);
    auto launchButton = new QPushButton(tr("Launch"), page);
    auto quitButton = new QPushButton(tr("Quit Running App"), page);
    auto directLaunchButton = new QPushButton(tr("Toggle Direct Launch"), page);
    auto hideButton = new QPushButton(tr("Hide / Unhide"), page);
    buttons->addWidget(backButton);
    buttons->addStretch();
    buttons->addWidget(hideButton);
    buttons->addWidget(directLaunchButton);
    buttons->addWidget(quitButton);
    buttons->addWidget(launchButton);
    layout->addLayout(buttons);

    connect(backButton, &QPushButton::clicked, this, &GuiNextWindow::showHostsPage);
    connect(launchButton, &QPushButton::clicked, this, &GuiNextWindow::launchSelectedApp);
    connect(quitButton, &QPushButton::clicked, this, &GuiNextWindow::quitRunningApp);
    connect(hideButton, &QPushButton::clicked, this, &GuiNextWindow::toggleSelectedAppHidden);
    connect(directLaunchButton, &QPushButton::clicked, this, &GuiNextWindow::toggleSelectedAppDirectLaunch);

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
    m_DefaultBitrateButton = new QPushButton(page);
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
    m_LanguageComboBox = new QComboBox(page);
    addComboItem(m_LanguageComboBox, tr("Automatic"), StreamingPreferences::LANG_AUTO);
    addComboItem(m_LanguageComboBox, QStringLiteral("Deutsch"), StreamingPreferences::LANG_DE);
    addComboItem(m_LanguageComboBox, QStringLiteral("English"), StreamingPreferences::LANG_EN);
    addComboItem(m_LanguageComboBox, QStringLiteral("Français"), StreamingPreferences::LANG_FR);
    addComboItem(m_LanguageComboBox, QStringLiteral("简体中文"), StreamingPreferences::LANG_ZH_CN);
    addComboItem(m_LanguageComboBox, QStringLiteral("Norwegian Bokmål"), StreamingPreferences::LANG_NB_NO);
    addComboItem(m_LanguageComboBox, QStringLiteral("русский"), StreamingPreferences::LANG_RU);
    addComboItem(m_LanguageComboBox, QStringLiteral("Español"), StreamingPreferences::LANG_ES);
    addComboItem(m_LanguageComboBox, QStringLiteral("日本語"), StreamingPreferences::LANG_JA);
    addComboItem(m_LanguageComboBox, QStringLiteral("Tiếng Việt"), StreamingPreferences::LANG_VI);
    addComboItem(m_LanguageComboBox, QStringLiteral("ภาษาไทย"), StreamingPreferences::LANG_TH);
    addComboItem(m_LanguageComboBox, QStringLiteral("한국어"), StreamingPreferences::LANG_KO);
    addComboItem(m_LanguageComboBox, QStringLiteral("Magyar"), StreamingPreferences::LANG_HU);
    addComboItem(m_LanguageComboBox, QStringLiteral("Nederlands"), StreamingPreferences::LANG_NL);
    addComboItem(m_LanguageComboBox, QStringLiteral("Svenska"), StreamingPreferences::LANG_SV);
    addComboItem(m_LanguageComboBox, QStringLiteral("Türkçe"), StreamingPreferences::LANG_TR);
    addComboItem(m_LanguageComboBox, QStringLiteral("繁體中文"), StreamingPreferences::LANG_ZH_TW);
    addComboItem(m_LanguageComboBox, QStringLiteral("Português"), StreamingPreferences::LANG_PT);
    addComboItem(m_LanguageComboBox, QStringLiteral("Português do Brasil"), StreamingPreferences::LANG_PT_BR);
    addComboItem(m_LanguageComboBox, QStringLiteral("Ελληνικά"), StreamingPreferences::LANG_EL);
    addComboItem(m_LanguageComboBox, QStringLiteral("Italiano"), StreamingPreferences::LANG_IT);
    addComboItem(m_LanguageComboBox, QStringLiteral("Język polski"), StreamingPreferences::LANG_PL);
    addComboItem(m_LanguageComboBox, QStringLiteral("Čeština"), StreamingPreferences::LANG_CS);
    addComboItem(m_LanguageComboBox, QStringLiteral("Български"), StreamingPreferences::LANG_BG);
    addComboItem(m_LanguageComboBox, QStringLiteral("தமிழ்"), StreamingPreferences::LANG_TA);
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
    form->addRow(QString(), m_DefaultBitrateButton);
    form->addRow(tr("Packet size"), m_PacketSizeSpinBox);
    form->addRow(tr("Audio"), m_AudioConfigComboBox);
    form->addRow(tr("Video codec"), m_VideoCodecComboBox);
    form->addRow(tr("Video decoder"), m_VideoDecoderComboBox);
    form->addRow(tr("Stream window mode"), m_WindowModeComboBox);
    form->addRow(tr("UI startup mode"), m_UiModeComboBox);
    form->addRow(tr("Capture system keys"), m_CaptureSysKeysComboBox);
    form->addRow(tr("Language"), m_LanguageComboBox);
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
    connect(m_DefaultBitrateButton, &QPushButton::clicked, this, &GuiNextWindow::resetBitrateToDefault);
    connect(m_BitrateSpinBox, qOverload<int>(&QSpinBox::valueChanged), this, &GuiNextWindow::handleBitrateEdited);
    connect(m_WidthSpinBox, qOverload<int>(&QSpinBox::valueChanged), this, &GuiNextWindow::handleStreamingShapeChanged);
    connect(m_HeightSpinBox, qOverload<int>(&QSpinBox::valueChanged), this, &GuiNextWindow::handleStreamingShapeChanged);
    connect(m_FpsSpinBox, qOverload<int>(&QSpinBox::valueChanged), this, &GuiNextWindow::handleStreamingShapeChanged);
    connect(m_Yuv444CheckBox, &QCheckBox::toggled, this, &GuiNextWindow::handleStreamingShapeChanged);
    connect(m_AutoAdjustBitrateCheckBox, &QCheckBox::toggled, this, [this](bool checked) {
        if (!m_LoadingSettings && checked) {
            resetBitrateToDefault();
        }
        else {
            updateDefaultBitrateButton();
        }
    });

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

    openHost(index, false, true);
}

void GuiNextWindow::openSelectedHostAllApps()
{
    int index = selectedHostIndex();
    if (index < 0) {
        return;
    }

    openHost(index, true, false);
}

void GuiNextWindow::openHost(int index, bool showHiddenGames, bool allowDirectLaunch)
{
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
    if (!computer.serverSupported) {
        showUnsupportedHostWarning(computer);
        return;
    }
    if (!computer.paired) {
        pairSelectedHost();
        return;
    }

    m_CurrentComputerIndex = index;
    m_AppList.reset(m_Facade.createAppList(index, showHiddenGames, this));
    if (m_AppList == nullptr) {
        return;
    }

    connect(m_AppList.data(), &AppListFacade::appsReset, this, &GuiNextWindow::refreshApps);
    connect(m_AppList.data(), &AppListFacade::appChanged, this, &GuiNextWindow::refreshApps);
    connect(m_AppList.data(), &AppListFacade::quitAppCompleted, this, &GuiNextWindow::handleQuitAppCompleted);
    connect(m_AppList.data(), &AppListFacade::computerLost, this, [this]() {
        QMessageBox::warning(this, tr("Host Lost"), tr("The selected host is no longer available."));
        showHostsPage();
    });

    if (allowDirectLaunch) {
        int directLaunchIndex = m_AppList->getDirectLaunchAppIndex();
        if (directLaunchIndex >= 0 && m_AppList->getRunningAppId() == 0) {
            FrontendApp app = m_AppList->appAt(directLaunchIndex);
            Session* session = m_AppList->createSessionForApp(directLaunchIndex);
            launchSession(session, app.name, false);
            return;
        }
    }

    m_AppHeaderLabel->setText(showHiddenGames ? tr("%1 - All Apps").arg(computer.name) : computer.name);
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
        QStringList tags;
        if (app.running) {
            tags.append(tr("running"));
        }
        if (app.hidden) {
            tags.append(tr("hidden"));
        }
        if (app.directLaunch) {
            tags.append(tr("direct launch"));
        }
        QString suffix = tags.isEmpty() ? QString() : QStringLiteral(" (%1)").arg(tags.join(QStringLiteral(", ")));
        auto item = new QListWidgetItem(app.name + suffix);
        if (!app.boxArt.isEmpty()) {
            item->setIcon(QIcon(iconPathForUrl(app.boxArt)));
        }
        item->setData(Qt::UserRole, i);
        if (app.hidden) {
            item->setForeground(Qt::gray);
        }
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
    if (m_QuitInProgress) {
        QMessageBox::information(this,
                                 tr("Quit Running App"),
                                 tr("A quit request is already in progress."));
        return;
    }

    FrontendApp app = m_AppList->appAt(appIndex);
    const int runningAppId = m_AppList->getRunningAppId();
    if (runningAppId != 0 && runningAppId != app.appId) {
        const QString runningAppName = m_AppList->getRunningAppName();
        const QMessageBox::StandardButton result = QMessageBox::question(
            this,
            tr("Quit Running App"),
            tr("Are you sure you want to quit %1? Any unsaved progress will be lost.").arg(runningAppName));
        if (result != QMessageBox::Yes) {
            return;
        }

        m_PendingLaunchAfterQuitIndex = appIndex;
        m_LaunchAfterQuit = true;
        m_QuitInProgress = true;
        setStatusText(tr("Quitting %1...").arg(runningAppName));
        m_AppList->quitRunningApp();
        return;
    }

    Session* session = m_AppList->createSessionForApp(appIndex);
    launchSession(session, app.name, runningAppId == app.appId);
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
    if (m_PairingInProgress) {
        QMessageBox::information(this,
                                 tr("Pair Host"),
                                 tr("Another pairing attempt is already in progress."));
        return;
    }

    int index = selectedHostIndex();
    if (index < 0) {
        return;
    }

    FrontendComputer computer = m_Facade.computers()->computerAt(index);
    if (!computer.online) {
        QMessageBox::information(this, tr("Host Offline"), tr("This host is not online."));
        return;
    }
    if (!computer.serverSupported) {
        showUnsupportedHostWarning(computer);
        return;
    }
    if (computer.paired) {
        QMessageBox::information(this, tr("Pair Host"), tr("This host is already paired."));
        return;
    }

    QString pin = m_Facade.computers()->generatePinString();
    m_PairingInProgress = true;
    setStatusText(tr("Pairing in progress..."));
    m_Facade.computers()->pairComputer(index, pin);

    auto dialog = new QMessageBox(QMessageBox::Information,
                                  tr("Pair Host"),
                                  tr("Enter this PIN on your host PC when prompted: %1").arg(pin),
                                  QMessageBox::Ok,
                                  this);
    dialog->setAttribute(Qt::WA_DeleteOnClose);
    m_PairingDialog = dialog;
    dialog->open();
}

void GuiNextWindow::wakeSelectedHost()
{
    int index = selectedHostIndex();
    if (index >= 0) {
        m_Facade.computers()->wakeComputer(index);
    }
}

void GuiNextWindow::testSelectedHostConnection()
{
    if (m_ConnectionTestInProgress) {
        if (!m_ConnectionTestDialog.isNull()) {
            m_ConnectionTestDialog->show();
            m_ConnectionTestDialog->raise();
            m_ConnectionTestDialog->activateWindow();
        }
        return;
    }

    int index = selectedHostIndex();
    if (index < 0) {
        return;
    }

    m_ConnectionTestInProgress = true;
    setStatusText(tr("Testing network connection..."));
    m_Facade.computers()->testConnectionForComputer(index);

    auto dialog = new QMessageBox(QMessageBox::Information,
                                  tr("Test Network"),
                                  tr("Moonlight is testing your network connection to determine if any required ports are blocked.\n\nThis may take a few seconds..."),
                                  QMessageBox::Ok,
                                  this);
    dialog->setAttribute(Qt::WA_DeleteOnClose);
    m_ConnectionTestDialog = dialog;
    dialog->open();
}

void GuiNextWindow::showSelectedHostDetails()
{
    int index = selectedHostIndex();
    if (index < 0) {
        return;
    }

    QMessageBox::information(this,
                             tr("Host Details"),
                             m_Facade.computers()->computerAt(index).details);
}

void GuiNextWindow::showUnsupportedHostWarning(const FrontendComputer& computer)
{
    QMessageBox::warning(this,
                         tr("Unsupported Host"),
                         tr("The version of GeForce Experience on %1 is not supported by this build of Moonlight. You must update Moonlight to stream from %1.").arg(computer.name));
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
    m_LoadingSettings = true;
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
    setComboValue(m_LanguageComboBox, preferences.language);
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
    m_LoadingSettings = false;
    updateDefaultBitrateButton();
    m_ControllerAdapter.setUiNavMode(true);
    m_Stack->setCurrentIndex(SettingsPageIndex);
}

void GuiNextWindow::openHelp()
{
    const FrontendSystemProperties system = m_Facade.system()->properties();
    if (!system.hasBrowser) {
        QMessageBox::information(this,
                                 tr("Help"),
                                 tr("No web browser is available to open the Moonlight setup guide."));
        return;
    }

    QDesktopServices::openUrl(QUrl(QStringLiteral("https://github.com/moonlight-stream/moonlight-docs/wiki/Setup-Guide")));
}

void GuiNextWindow::openDiscord()
{
    const FrontendSystemProperties system = m_Facade.system()->properties();
    if (!system.hasBrowser) {
        QMessageBox::information(this,
                                 tr("Discord"),
                                 tr("No web browser is available to open the Moonlight Discord community."));
        return;
    }

    QDesktopServices::openUrl(QUrl(QStringLiteral("https://moonlight-stream.org/discord")));
}

void GuiNextWindow::saveSettings()
{
    FrontendStreamingPreferences preferences = m_Facade.preferences()->preferences();
    const int previousLanguage = preferences.language;
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
    preferences.language = comboValue(m_LanguageComboBox);
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
    if (preferences.language != previousLanguage) {
        m_Facade.preferences()->retranslate();
        QMessageBox::information(this,
                                 tr("Language"),
                                 tr("Restart Moonlight for the language change to fully take effect."));
    }
    showHostsPage();
}

void GuiNextWindow::handleStreamingShapeChanged()
{
    if (m_LoadingSettings) {
        return;
    }

    if (m_AutoAdjustBitrateCheckBox->isChecked()) {
        resetBitrateToDefault();
        return;
    }

    updateDefaultBitrateButton();
}

void GuiNextWindow::handleBitrateEdited()
{
    if (m_LoadingSettings) {
        return;
    }

    const int defaultBitrate = defaultBitrateForCurrentSettings();
    if (m_BitrateSpinBox->value() != defaultBitrate && m_AutoAdjustBitrateCheckBox->isChecked()) {
        QSignalBlocker blocker(m_AutoAdjustBitrateCheckBox);
        m_AutoAdjustBitrateCheckBox->setChecked(false);
    }
    updateDefaultBitrateButton();
}

void GuiNextWindow::resetBitrateToDefault()
{
    const int defaultBitrate = defaultBitrateForCurrentSettings();
    {
        QSignalBlocker blocker(m_BitrateSpinBox);
        m_BitrateSpinBox->setValue(defaultBitrate);
    }
    if (!m_AutoAdjustBitrateCheckBox->isChecked()) {
        QSignalBlocker blocker(m_AutoAdjustBitrateCheckBox);
        m_AutoAdjustBitrateCheckBox->setChecked(true);
    }
    updateDefaultBitrateButton();
}

int GuiNextWindow::defaultBitrateForCurrentSettings()
{
    return m_Facade.preferences()->getDefaultBitrate(m_WidthSpinBox->value(),
                                                     m_HeightSpinBox->value(),
                                                     m_FpsSpinBox->value(),
                                                     m_Yuv444CheckBox->isChecked());
}

void GuiNextWindow::updateDefaultBitrateButton()
{
    const int defaultBitrate = defaultBitrateForCurrentSettings();
    m_DefaultBitrateButton->setText(tr("Use Default (%1 Mbps)").arg(defaultBitrate / 1000.0));
    m_DefaultBitrateButton->setEnabled(m_BitrateSpinBox->value() != defaultBitrate ||
                                       !m_AutoAdjustBitrateCheckBox->isChecked());
}

void GuiNextWindow::showHostsPage()
{
    m_ControllerAdapter.setUiNavMode(false);
    m_Stack->setCurrentIndex(HostPageIndex);
    refreshHosts();
}

void GuiNextWindow::quitRunningApp()
{
    if (m_AppList == nullptr) {
        return;
    }

    if (m_QuitInProgress) {
        QMessageBox::information(this,
                                 tr("Quit Running App"),
                                 tr("A quit request is already in progress."));
        return;
    }

    const int runningAppId = m_AppList->getRunningAppId();
    if (runningAppId == 0) {
        QMessageBox::information(this,
                                 tr("Quit Running App"),
                                 tr("No app is currently running on this host."));
        return;
    }

    const QString runningAppName = m_AppList->getRunningAppName();
    const QMessageBox::StandardButton result = QMessageBox::question(
        this,
        tr("Quit Running App"),
        tr("Are you sure you want to quit %1? Any unsaved progress will be lost.").arg(runningAppName));
    if (result != QMessageBox::Yes) {
        return;
    }

    m_PendingLaunchAfterQuitIndex = -1;
    m_LaunchAfterQuit = false;
    m_QuitInProgress = true;
    setStatusText(tr("Quitting %1...").arg(runningAppName));
    m_AppList->quitRunningApp();
}

void GuiNextWindow::handleQuitAppCompleted(const QString& error)
{
    m_QuitInProgress = false;

    if (!error.isEmpty()) {
        m_LaunchAfterQuit = false;
        m_PendingLaunchAfterQuitIndex = -1;
        QMessageBox::warning(this, tr("Quit Running App"), error);
        refreshApps();
        return;
    }

    setStatusText(tr("App quit successfully."));
    refreshApps();

    if (!m_LaunchAfterQuit) {
        return;
    }

    const int appIndex = m_PendingLaunchAfterQuitIndex;
    m_LaunchAfterQuit = false;
    m_PendingLaunchAfterQuitIndex = -1;

    if (m_AppList == nullptr || appIndex < 0 || appIndex >= m_AppList->count()) {
        QMessageBox::warning(this,
                             tr("Stream Error"),
                             tr("Unable to start stream: the selected app is no longer available."));
        return;
    }

    FrontendApp app = m_AppList->appAt(appIndex);
    Session* session = m_AppList->createSessionForApp(appIndex);
    launchSession(session, app.name, false);
}

void GuiNextWindow::handlePairingCompleted(const QString& error)
{
    m_PairingInProgress = false;
    if (!m_PairingDialog.isNull()) {
        m_PairingDialog->close();
        m_PairingDialog.clear();
    }

    if (error.isEmpty()) {
        setStatusText(tr("Pairing completed."));
    }
    else {
        QMessageBox::warning(this, tr("Pairing Failed"), error);
    }
    refreshHosts();
}

void GuiNextWindow::handleConnectionTestCompleted(int result, const QString& blockedPorts)
{
    m_ConnectionTestInProgress = false;

    QString message;
    QMessageBox::Icon icon = QMessageBox::Information;
    if (result == -1) {
        icon = QMessageBox::Warning;
        message = tr("The network test could not be performed because none of Moonlight's connection testing servers were reachable from this PC. Check your Internet connection or try again later.");
    }
    else if (result == 0) {
        message = tr("This network does not appear to be blocking Moonlight. If you still have trouble connecting, check your PC's firewall settings.") +
                  QStringLiteral("\n\n") +
                  tr("If you are trying to stream over the Internet, install the Moonlight Internet Hosting Tool on your gaming PC and run the included Internet Streaming Tester to check your gaming PC's Internet connection.");
    }
    else {
        icon = QMessageBox::Critical;
        message = tr("Your PC's current network connection seems to be blocking Moonlight. Streaming over the Internet may not work while connected to this network.") +
                  QStringLiteral("\n\n") +
                  tr("The following network ports were blocked:") +
                  QStringLiteral("\n") +
                  blockedPorts;
    }

    setStatusText(tr("Network test completed."));
    if (m_ConnectionTestDialog.isNull()) {
        auto dialog = new QMessageBox(icon, tr("Test Network"), message, QMessageBox::Ok, this);
        dialog->setAttribute(Qt::WA_DeleteOnClose);
        m_ConnectionTestDialog = dialog;
        dialog->open();
        return;
    }

    m_ConnectionTestDialog->setIcon(icon);
    m_ConnectionTestDialog->setText(message);
}

void GuiNextWindow::toggleSelectedAppHidden()
{
    if (m_AppList == nullptr) {
        return;
    }

    int appIndex = selectedAppIndex();
    if (appIndex < 0) {
        return;
    }

    FrontendApp app = m_AppList->appAt(appIndex);
    if (!app.hidden && (app.running || app.directLaunch)) {
        QMessageBox::information(this,
                                 tr("Hide Game"),
                                 tr("Running or direct-launch games cannot be hidden."));
        return;
    }

    m_AppList->setAppHidden(appIndex, !app.hidden);
}

void GuiNextWindow::toggleSelectedAppDirectLaunch()
{
    if (m_AppList == nullptr) {
        return;
    }

    int appIndex = selectedAppIndex();
    if (appIndex < 0) {
        return;
    }

    FrontendApp app = m_AppList->appAt(appIndex);
    if (app.hidden) {
        QMessageBox::information(this,
                                 tr("Direct Launch"),
                                 tr("Hidden games cannot be used for direct launch."));
        return;
    }

    m_AppList->setAppDirectLaunch(appIndex, !app.directLaunch);
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
        if (QMessageBox::question(this, tr("Quit Moonlight"), tr("Are you sure you want to quit?")) == QMessageBox::Yes) {
            qApp->quit();
        }
        event->accept();
        return;
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
