#include "widgetshell.h"

#include "frontend/applistfacade.h"
#include "settings/streamingpreferences.h"

#include <QApplication>
#include <QCheckBox>
#include <QCloseEvent>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QInputDialog>
#include <QKeyEvent>
#include <QLabel>
#include <QMessageBox>
#include <QPushButton>
#include <QSpinBox>
#include <QVBoxLayout>

namespace {
    constexpr int HostPageIndex = 0;
    constexpr int AppPageIndex = 1;
    constexpr int SettingsPageIndex = 2;
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
    auto title = new QLabel(tr("Moonlight - GUI Next"), page);
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

void GuiNextWindow::buildSettingsPage()
{
    auto page = new QWidget(this);
    auto layout = new QVBoxLayout(page);
    auto title = new QLabel(tr("Settings"), page);
    title->setStyleSheet(QStringLiteral("font-size: 24px; font-weight: bold;"));
    layout->addWidget(title);

    auto form = new QFormLayout();
    m_WidthSpinBox = new QSpinBox(page);
    m_WidthSpinBox->setRange(1, 16384);
    m_HeightSpinBox = new QSpinBox(page);
    m_HeightSpinBox->setRange(1, 16384);
    m_FpsSpinBox = new QSpinBox(page);
    m_FpsSpinBox->setRange(10, 480);
    m_BitrateSpinBox = new QSpinBox(page);
    m_BitrateSpinBox->setRange(500, 500000);
    m_BitrateSpinBox->setSuffix(tr(" Kbps"));
    m_HdrCheckBox = new QCheckBox(page);
    m_Yuv444CheckBox = new QCheckBox(page);
    form->addRow(tr("Width"), m_WidthSpinBox);
    form->addRow(tr("Height"), m_HeightSpinBox);
    form->addRow(tr("FPS"), m_FpsSpinBox);
    form->addRow(tr("Bitrate"), m_BitrateSpinBox);
    form->addRow(tr("HDR"), m_HdrCheckBox);
    form->addRow(tr("YUV 4:4:4"), m_Yuv444CheckBox);
    layout->addLayout(form);

    auto buttons = new QHBoxLayout();
    auto backButton = new QPushButton(tr("Back"), page);
    auto saveButton = new QPushButton(tr("Save"), page);
    buttons->addWidget(backButton);
    buttons->addStretch();
    buttons->addWidget(saveButton);
    layout->addLayout(buttons);
    layout->addStretch();

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
}

void GuiNextWindow::setStatusText(const QString& text)
{
    if (m_StatusLabel != nullptr) {
        m_StatusLabel->setText(text);
    }
}
