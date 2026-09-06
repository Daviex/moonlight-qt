#include <QtTest>
#include <QTemporaryDir>
#include <QQmlComponent>
#include <QQmlContext>
#include <QQmlExpression>
#include <QQuickItem>
#include <QQuickWindow>
#include <QPointer>
#include "settings/gamestreamingsettings.h"
#include "backend/profilemanager.h"
#include "cli/commandlineparser.h"

// Only storage scope and platform probes are faked. The resolver, serializer,
// CLI parser and all settings/navigation QML are the production implementations.
static QString activeProfile = "default";
QString ProfileManager::activeProfileId() { return activeProfile; }
void ProfileManager::beginProfileSettings(QSettings& settings) { beginProfileSettings(settings, activeProfile); }
void ProfileManager::beginProfileSettings(QSettings& settings, QString profileId) { settings.beginGroup("profiles/" + profileId); }
namespace WMUtils {
bool isRunningWayland() { return false; }
bool isGpuSlow() { return false; }
}

class GameSettingsFactory : public QObject
{
    Q_OBJECT
public:
    Q_INVOKABLE GameStreamingSettings* create(int appId) { return new GameStreamingSettings(activeProfile, "host-a", appId, this); }
    Q_INVOKABLE bool remove(int appId) { return GameStreamingSettings::remove(activeProfile, "host-a", appId); }
};

class GameSettingsTest : public QObject
{
    Q_OBJECT
    QTemporaryDir m_SettingsDirectory;
    GameSettingsFactory m_Factory;
    std::unique_ptr<QQmlEngine> m_Engine;
    std::unique_ptr<QQuickWindow> m_Window;

    StreamingPreferences* base() { return StreamingPreferences::get(); }
    QVariantMap stored(int appId = 1, QString host = "host-a", QString profile = "default") {
        return GameStreamingSettings::load(profile, host, appId);
    }
    QVariant evaluate(const QString& code, QObject* scope = nullptr) {
        if (!scope) scope = m_Window.get();
        QQmlExpression expression(qmlContext(scope), scope, code);
        auto result = expression.evaluate();
        if (expression.hasError()) qFatal("%s", qPrintable(expression.error().toString()));
        return result;
    }
    bool busy() { return evaluate("stackView.busy").toBool(); }
    QObject* page() { return evaluate("stackView.currentItem").value<QObject*>(); }
    void openEditor() {
        m_Engine = std::make_unique<QQmlEngine>();
        m_Engine->rootContext()->setContextProperty("initialView", "qrc:/gui/ProfileSelectionView.qml");
        m_Engine->rootContext()->setContextProperty("runConfigChecks", false);
        m_Engine->rootContext()->setContextProperty("gameSettingsFactory", &m_Factory);
        QQmlComponent component(m_Engine.get(), QUrl("qrc:/gui/main.qml"));
        QVERIFY2(component.isReady(), qPrintable(component.errorString()));
        m_Window.reset(qobject_cast<QQuickWindow*>(component.create()));
        QVERIFY(m_Window);
        m_Window->requestActivate();
        QTRY_VERIFY(m_Window->isActive());
        QTRY_VERIFY(!busy());
        evaluate("stackView.currentItem.openComputer(0, 'Test PC', false)");
        QTRY_VERIFY(!busy());
        evaluate("stackView.currentItem.openGameSettings(1, 'Test Game')");
        QTRY_VERIFY(!busy());
        QCOMPARE(evaluate("stackView.depth").toInt(), 4);
    }

private slots:
    void initTestCase() {
        QGuiApplication::setFont(QFont("Segoe UI", 10));
        QVERIFY(m_SettingsDirectory.isValid());
        QCoreApplication::setOrganizationName("MoonlightTests");
        QCoreApplication::setApplicationName("GameSettings");
        QSettings::setDefaultFormat(QSettings::IniFormat);
        QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, m_SettingsDirectory.path());
        const QUrl backend("qrc:/navigation-tests/mocks/Backend.qml");
        for (const auto name : {"ProfileManager", "ComputerManager", "SdlGamepadKeyNavigation", "SystemProperties", "AutoUpdateChecker"})
            qmlRegisterSingletonType(backend, name, 1, 0, name);
        qmlRegisterSingletonType<StreamingPreferences>("StreamingPreferences", 1, 0, "StreamingPreferences",
            [](QQmlEngine*, QJSEngine*) -> QObject* {
                auto preferences = StreamingPreferences::get();
                QQmlEngine::setObjectOwnership(preferences, QQmlEngine::CppOwnership);
                return preferences;
            });
        qmlRegisterUncreatableType<GameStreamingSettings>("GameStreamingSettings", 1, 0, "GameStreamingSettings", "Test factory");
        qmlRegisterType(QUrl("qrc:/navigation-tests/mocks/ComputerModel.qml"), "ComputerModel", 1, 0, "ComputerModel");
        qmlRegisterType(QUrl("qrc:/navigation-tests/mocks/AppModel.qml"), "AppModel", 1, 0, "AppModel");
    }

    void init() {
        activeProfile = "default";
        QSettings settings;
        settings.clear();
        base()->reload();
    }
    void cleanup() {
        m_Window.reset();
        m_Engine.reset();
        QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
    }

    void noOverridesPreserveExactProfile() {
        base()->bitrateKbps = 43210;
        base()->packetSize = 1200;
        auto resolved = GameStreamingSettings::resolve(*base(), activeProfile, "host-a", 1);
        QCOMPARE(resolved->gameValues(), base()->gameValues());
        QCOMPARE(resolved->packetSize, 1200);
        resolved->quitAppAfter = true;
        resolved->save();
        QVERIFY(!base()->quitAppAfter);
        QVERIFY(QSettings().allKeys().isEmpty());
        GameStreamingSettings editor(activeProfile, "host-a", 1);
        QVERIFY(editor.save());
        QVERIFY(QSettings().allKeys().isEmpty());
    }

    void sparseOverridesAndIsolation() {
        GameStreamingSettings editor(activeProfile, "host-a", 1);
        editor.preferences()->fps = 120;
        editor.preferences()->enableVsync = false;
        QVERIFY(editor.save());
        QCOMPARE(stored(), (QVariantMap{{"fps", 120}, {"vsync", false}}));
        QVERIFY(stored(2).isEmpty());
        QVERIFY(stored(1, "host-b").isEmpty());
        QVERIFY(stored(1, "host-a", "second").isEmpty());
        QCOMPARE(base()->fps, 60);
        QVERIFY(base()->enableVsync);
        base()->width = 2560;
        base()->height = 1440;
        auto effective = GameStreamingSettings::resolve(*base(), activeProfile, "host-a", 1);
        QCOMPARE(effective->width, 2560);
        QCOMPARE(effective->height, 1440);
        QCOMPARE(effective->fps, 120);
        QCOMPARE(effective->bitrateKbps, base()->getDefaultBitrate(2560, 1440, 120, false));
        activeProfile = "second";
        base()->reload();
        base()->fps = 90;
        GameStreamingSettings second(activeProfile, "host-a", 1);
        QCOMPARE(second.preferences()->fps, 90);
        second.preferences()->fps = 144;
        QVERIFY(second.save());
        QCOMPARE(stored(1, "host-a", "second").value("fps"), QVariant(144));
        QCOMPARE(stored().value("fps"), QVariant(120));
    }

    void resolutionIsAtomicAndResetIsIdempotent() {
        GameStreamingSettings editor(activeProfile, "host-a", 1);
        editor.preferences()->width = 1920;
        editor.preferences()->fps = 120;
        QVERIFY(editor.save());
        QCOMPARE(stored().value("height").toInt(), 720);
        editor.reset("width");
        QVERIFY(editor.save());
        QCOMPARE(stored(), (QVariantMap{{"fps", 120}}));
        editor.reset();
        QVERIFY(editor.save());
        QVERIFY(editor.save());
        QVERIFY(stored().isEmpty());
        QVERIFY(QSettings().allKeys().isEmpty());
        QCOMPARE(editor.preferences()->gameValues(), base()->gameValues());
    }

    void manualAndAutomaticBitrate() {
        base()->autoAdjustBitrate = false;
        base()->bitrateKbps = 25000;
        GameStreamingSettings editor(activeProfile, "host-a", 1);
        editor.preferences()->fps = 120;
        QVERIFY(editor.save());
        auto effective = GameStreamingSettings::resolve(*base(), activeProfile, "host-a", 1);
        QCOMPARE(effective->bitrateKbps, 25000);
        editor.preferences()->bitrateKbps = 50000;
        QVERIFY(editor.save());
        QCOMPARE(stored().value("autoadjustbitrate"), QVariant(false));
        QCOMPARE(stored().value("bitrate"), QVariant(50000));
        editor.preferences()->autoAdjustBitrate = true;
        QVERIFY(editor.save());
        QVERIFY(!stored().contains("bitrate"));
        QCOMPARE(stored().value("autoadjustbitrate"), QVariant(true));
        effective = GameStreamingSettings::resolve(*base(), activeProfile, "host-a", 1);
        QCOMPARE(effective->bitrateKbps, base()->getDefaultBitrate(1280, 720, 120, false));
        editor.reset("bitrate");
        QVERIFY(editor.save());
        effective = GameStreamingSettings::resolve(*base(), activeProfile, "host-a", 1);
        QCOMPARE(effective->bitrateKbps, 25000);
    }

    void unchangedOverrideSurvivesMatchingProfile() {
        GameStreamingSettings editor(activeProfile, "host-a", 1);
        editor.preferences()->fps = 120;
        QVERIFY(editor.save());
        base()->fps = 120;
        GameStreamingSettings reopened(activeProfile, "host-a", 1);
        QVERIFY(reopened.save());
        QCOMPARE(stored().value("fps"), QVariant(120));
        reopened.preferences()->setProperty("fps", 60);
        reopened.preferences()->setProperty("fps", 120);
        QVERIFY(reopened.save());
        QVERIFY(stored().isEmpty());
        reopened.preferences()->fps = 60;
        QVERIFY(reopened.save());
        reopened.preferences()->fps = 120;
        QVERIFY(reopened.save());
        QVERIFY(stored().isEmpty());
    }

    void invalidValuesAndScopesAreRejected() {
        auto valid = StreamingPreferences::validatedGameValues({{"width", 1920}, {"height", -1},
            {"fps", 0}, {"vsync", "maybe"}, {"audiocfg", 99}, {"videocfg", 3},
            {"renderer", -1}, {"hdr", false}, {"captureSysKeys", 0}, {"language", 1},
            {"mdns", false}, {"uidisplaymode", 2}, {"richpresence", false}});
        QCOMPARE(valid, (QVariantMap{{"hdr", false}}));
        QVERIFY(!GameStreamingSettings::remove("../default", "host-a", 1));
        QVERIFY(!GameStreamingSettings::remove("default", "../host-a", 1));
        GameStreamingSettings invalid("default", "host-a", -1);
        invalid.preferences()->fps = 120;
        QVERIFY(!invalid.save());
        QVERIFY(QSettings().allKeys().isEmpty());
    }

    void staleEditorAndHostDeletion() {
        GameStreamingSettings editor(activeProfile, "host-a", 1);
        editor.preferences()->fps = 120;
        activeProfile = "second";
        QVERIFY(!editor.save());
        activeProfile = "default";
        QVERIFY(editor.save());
        GameStreamingSettings other(activeProfile, "host-b", 1);
        other.preferences()->fps = 90;
        QVERIFY(other.save());
        editor.invalidate();
        GameStreamingSettings::removeHost(activeProfile, "host-a");
        editor.preferences()->fps = 144;
        QVERIFY(!editor.save());
        QVERIFY(stored().isEmpty());
        QCOMPARE(stored(1, "host-b").value("fps"), QVariant(90));
    }

    void cliExplicitValuesWin() {
        GameStreamingSettings editor(activeProfile, "host-a", 1);
        editor.preferences()->fps = 120;
        editor.preferences()->enableVsync = false;
        QVERIFY(editor.save());
        auto effective = GameStreamingSettings::resolve(*base(), activeProfile, "host-a", 1);
        StreamCommandLineParser parser;
        parser.parse({"moonlight", "stream", "host-a", "Game", "--fps", "60", "--vsync", "--yuv444"}, effective.get());
        QCOMPARE(effective->fps, 60);
        QVERIFY(effective->enableVsync);
        QCOMPARE(effective->bitrateKbps, base()->getDefaultBitrate(1280, 720, 60, true));
        parser.parse({"moonlight", "stream", "host-a", "Game", "--fps", "90", "--bitrate", "27000"}, effective.get());
        QCOMPARE(effective->bitrateKbps, 27000);
        effective->autoAdjustBitrate = false;
        parser.parse({"moonlight", "stream", "host-a", "Game", "--no-yuv444"}, effective.get());
        QCOMPARE(effective->bitrateKbps, 27000);
        QCOMPARE(stored().value("fps"), QVariant(120));
    }

    void allSupportedFieldsRoundTrip() {
        auto values = base()->gameValues();
        for (auto it = values.begin(); it != values.end(); ++it) {
            if (it.value().userType() == QMetaType::Bool) it.value() = !it.value().toBool();
        }
        values["width"] = 1920;
        values["height"] = 1080;
        values["fps"] = 144;
        values["bitrate"] = 20000;
        values["autoadjustbitrate"] = false;
        values["audiocfg"] = 2;
        values["videocfg"] = 4;
        values["videodec"] = 2;
        values["renderer"] = 1;
        values["windowmode"] = 2;
        values["capturesyskeys"] = 2;
        GameStreamingSettings editor(activeProfile, "host-a", 1);
        editor.preferences()->applyGameValues(values);
        QVERIFY(editor.save());
        const auto effective = GameStreamingSettings::resolve(*base(), activeProfile, "host-a", 1);
        QCOMPARE(effective->gameValues(), values);
        QCOMPARE(effective->uiDisplayMode, base()->uiDisplayMode);
        QCOMPARE(effective->language, base()->language);
        QCOMPARE(effective->richPresence, base()->richPresence);
    }

    void qmlOpenAndCloseDoesNotCustomize() {
        base()->enableVsync = false;
        base()->framePacing = true;
        base()->bitrateKbps = 43210;
        const auto before = base()->gameValues();
        openEditor();
        QCOMPARE(evaluate("editor.customSettings.length", page()).toInt(), 0);
        QTest::keyClick(m_Window.get(), Qt::Key_Escape);
        QTRY_VERIFY(!busy());
        QCOMPARE(evaluate("stackView.depth").toInt(), 3);
        QVERIFY(stored().isEmpty());
        QCOMPARE(base()->gameValues(), before);
        QVERIFY(qobject_cast<QQuickItem*>(page())->hasActiveFocus());
        QVERIFY(!evaluate("SdlGamepadKeyNavigation.uiNavMode").toBool());
    }

    void qmlChangeSaveAndControllerReset() {
        openEditor();
        QPointer<QObject> editor = evaluate("editor", page()).value<QObject*>();
        evaluate("settingsLoader.item.preferences.fps = 120", page());
        QCOMPARE(evaluate("editor.customSettings.length", page()).toInt(), 1);
        // Reach the footer by the same Tab traversal generated by controller navigation.
        bool reached = false;
        for (int i = 0; i < 100; ++i) {
            QTest::keyClick(m_Window.get(), Qt::Key_Tab);
            if (m_Window->activeFocusItem() && m_Window->activeFocusItem()->objectName() == "resetGameSetting") {
                reached = true;
                break;
            }
        }
        QVERIFY(reached);
        QTest::keyClick(m_Window.get(), Qt::Key_Space);
        QTRY_COMPARE(evaluate("editor.customSettings.length", page()).toInt(), 0);
        evaluate("settingsLoader.item.preferences.fps = 144", page());
        for (int i = 0; i < 12; ++i) QTest::keyClick(m_Window.get(), Qt::Key_Escape);
        QTRY_VERIFY(!busy());
        QCOMPARE(evaluate("stackView.depth").toInt(), 3);
        QCOMPARE(stored().value("fps"), QVariant(144));
        QCOMPARE(page()->property("currentIndex").toInt(), 0);
        QVERIFY(qobject_cast<QQuickItem*>(page())->hasActiveFocus());
        QTRY_VERIFY(editor.isNull());
    }

    void qmlRealControlsAndResetAll() {
        openEditor();
        auto settings = evaluate("settingsLoader.item", page()).value<QObject*>();
        settings->findChild<QQuickItem*>("vsyncCheck")->forceActiveFocus();
        QTest::keyClick(m_Window.get(), Qt::Key_Space);
        QVERIFY(!evaluate("preferences.enableVsync", settings).toBool());
        QVERIFY(base()->enableVsync);
        settings->findChild<QQuickItem*>("videoBitrateSlider")->forceActiveFocus();
        QTest::keyClick(m_Window.get(), Qt::Key_Right);
        QVERIFY(!evaluate("preferences.autoAdjustBitrate", settings).toBool());
        settings->findChild<QQuickItem*>("captureSysKeysCheck")->forceActiveFocus();
        QTest::keyClick(m_Window.get(), Qt::Key_Space);
        QCOMPARE(evaluate("preferences.captureSysKeysMode", settings).toInt(), int(StreamingPreferences::CSK_FULLSCREEN));
        QVERIFY(!settings->findChild<QQuickItem*>("languageComboBox")->isVisible());
        QVERIFY(!settings->findChild<QQuickItem*>("uiDisplayModeComboBox")->isVisible());
        QVERIFY(!settings->findChild<QQuickItem*>("enableMdns")->isVisible());
        QVERIFY(!evaluate("settingsButton.visible").toBool());
        auto button = page()->findChild<QQuickItem*>("resetAllGameSettings");
        QVERIFY(button);
        button->forceActiveFocus(Qt::TabFocusReason);
        QTest::keyClick(m_Window.get(), Qt::Key_Space);
        QTRY_VERIFY(evaluate("resetDialog.opened", page()).toBool());
        QTest::keyClick(m_Window.get(), Qt::Key_Escape);
        QTRY_VERIFY(!evaluate("resetDialog.visible", page()).toBool());
        QVERIFY(evaluate("editor.customSettings.length", page()).toInt() > 0);
        button->forceActiveFocus(Qt::TabFocusReason);
        QTest::keyClick(m_Window.get(), Qt::Key_Space);
        QTRY_VERIFY(evaluate("resetDialog.opened", page()).toBool());
        evaluate("resetDialog.standardButton(Dialog.Yes).forceActiveFocus()", page());
        QTest::keyClick(m_Window.get(), Qt::Key_Space);
        QTRY_COMPARE(evaluate("editor.customSettings.length", page()).toInt(), 0);
        QTest::keyClick(m_Window.get(), Qt::Key_Escape);
        QTRY_VERIFY(!busy());
        QVERIFY(stored().isEmpty());
    }

    void qmlContextMenuAndSelectionRestore() {
        openEditor();
        QTest::keyClick(m_Window.get(), Qt::Key_Escape);
        QTRY_VERIFY(!busy());
        auto games = page();
        // Open the actual menu through controller X (Key_Menu), not by calling the editor directly.
        QTest::keyClick(m_Window.get(), Qt::Key_Menu);
        QTRY_VERIFY(evaluate("currentItem.appContextMenu.opened", games).toBool());
        evaluate("currentItem.appContextMenu.itemAt(2).forceActiveFocus()", games);
        QTest::keyClick(m_Window.get(), Qt::Key_Return);
        QTRY_VERIFY(!busy());
        QCOMPARE(evaluate("stackView.depth").toInt(), 4);
        evaluate("settingsLoader.item.preferences.fps = 120", page());
        // Simulate host refresh inserting another game ahead of the edited one.
        evaluate("appModel.insert(0, {appid: 2, name: 'Another Game', running: false, hidden: false, directLaunch: false, isAppCollectorGame: false, customStreamingSettings: false, boxart: 'qrc:/res/no_app_image.png'})", games);
        QTest::keyClick(m_Window.get(), Qt::Key_Escape);
        QTRY_VERIFY(!busy());
        QCOMPARE(page()->property("currentIndex").toInt(), 1);
        QCOMPARE(evaluate("currentItem.appContextMenu.initiator.grid.currentIndex", games).toInt(), 1);
        evaluate("appModel.setProperty(1, 'customStreamingSettings', true)", games);
        QTest::keyClick(m_Window.get(), Qt::Key_Menu);
        QTRY_VERIFY(evaluate("currentItem.appContextMenu.opened", games).toBool());
        evaluate("currentItem.appContextMenu.itemAt(3).forceActiveFocus()", games);
        QTest::keyClick(m_Window.get(), Qt::Key_Return);
        QTRY_VERIFY(evaluate("removeSettingsDialog.opened", games).toBool());
        evaluate("removeSettingsDialog.standardButton(Dialog.Yes).forceActiveFocus()", games);
        QTest::keyClick(m_Window.get(), Qt::Key_Return);
        QTRY_VERIFY(!evaluate("removeSettingsDialog.visible", games).toBool());
        QVERIFY(stored().isEmpty());
        QTRY_VERIFY(qobject_cast<QQuickItem*>(games)->hasActiveFocus());
        QTest::keyClick(m_Window.get(), Qt::Key_Left);
        QCOMPARE(games->property("currentIndex").toInt(), 0);
    }

    void qmlWindowCloseSavesAndLayout() {
        openEditor();
        auto settings = evaluate("settingsLoader.item", page()).value<QObject*>();
        settings->findChild<QQuickItem*>("fpsComboBox")->forceActiveFocus();
        QTest::keyClick(m_Window.get(), Qt::Key_Left);
        QCOMPARE(evaluate("preferences.fps", settings).toInt(), 30);
        const QString directory = qEnvironmentVariable("GAME_SETTINGS_SCREENSHOTS");
        if (!directory.isEmpty()) {
            for (const QSize size : {QSize(1280, 720), QSize(854, 600)}) {
                m_Window->resize(size);
                QTest::qWait(150);
                const auto screenshot = m_Window->grabWindow();
                QVERIFY(!screenshot.isNull());
                QVERIFY(screenshot.save(directory + QString("/settings-%1.png").arg(size.width())));
            }
        }
        m_Window.reset();
        QCOMPARE(stored().value("fps"), QVariant(30));
    }
};

QTEST_MAIN(GameSettingsTest)
#include "tst_game_settings.moc"
