#include <QtTest>
#include <QQmlComponent>
#include <QQmlContext>
#include <QQmlEngine>
#include <QQmlExpression>
#include <QQuickItem>
#include <QQuickWindow>
#include <QPointer>
#include <memory>

class NavigationTest : public QObject
{
    Q_OBJECT

private:
    std::unique_ptr<QQmlEngine> m_Engine;
    std::unique_ptr<QQuickWindow> m_Window;

    QVariant evaluate(const QString& code, QObject* scope = nullptr)
    {
        if (!scope) scope = m_Window.get();
        QQmlExpression expression(qmlContext(scope), scope, code);
        const auto result = expression.evaluate();
        if (expression.hasError()) qFatal("%s", qPrintable(expression.error().toString()));
        return result;
    }

    bool busy() { return evaluate("stackView.busy").toBool(); }
    int depth() { return evaluate("stackView.depth").toInt(); }
    QObject* page() { return evaluate("stackView.currentItem").value<QObject*>(); }

    QQuickItem* firstGridDelegate(QObject* grid)
    {
        auto content = grid->property("contentItem").value<QQuickItem*>();
        if (!content) return nullptr;
        for (auto child : content->childItems()) {
            if (child->property("grid").value<QObject*>() == grid && child->x() == 0 && child->y() == 0) {
                return child;
            }
        }
        return nullptr;
    }

    void openGames()
    {
        evaluate("stackView.currentItem.openComputer(0, 'Test PC', false)");
        QTRY_VERIFY(!busy());
        QCOMPARE(depth(), 3);
    }

private slots:
    void initTestCase()
    {
        const QUrl backend("qrc:/navigation-tests/mocks/Backend.qml");
        for (const auto name : {"ProfileManager", "ComputerManager", "StreamingPreferences",
                                "SdlGamepadKeyNavigation", "SystemProperties", "AutoUpdateChecker",
                                "PyroWaveCalibrator", "NetworkBuffers"}) {
            qmlRegisterSingletonType(backend, name, 1, 0, name);
        }
        qmlRegisterType(QUrl("qrc:/navigation-tests/mocks/ComputerModel.qml"), "ComputerModel", 1, 0, "ComputerModel");
        qmlRegisterType(QUrl("qrc:/navigation-tests/mocks/AppModel.qml"), "AppModel", 1, 0, "AppModel");
    }

    void init()
    {
        m_Engine = std::make_unique<QQmlEngine>();
        m_Engine->rootContext()->setContextProperty("initialView", "qrc:/gui/ProfileSelectionView.qml");
        m_Engine->rootContext()->setContextProperty("isGuiLaunch", true);
        QQmlComponent component(m_Engine.get(), QUrl("qrc:/gui/main.qml"));
        QVERIFY2(component.isReady(), qPrintable(component.errorString()));
        m_Window.reset(qobject_cast<QQuickWindow*>(component.create()));
        QVERIFY(m_Window);
        m_Window->requestActivate();
        QTRY_VERIFY(m_Window->isActive());
        QTRY_VERIFY(!busy());
        QCOMPARE(depth(), 2);
    }

    void cleanup()
    {
        m_Window.reset();
        m_Engine.reset();
    }

    void initialHostHasFocus()
    {
        auto hostPage = qobject_cast<QQuickItem*>(page());
        QVERIFY(hostPage);
        QTRY_VERIFY(hostPage->hasActiveFocus());
        QTest::keyClick(m_Window.get(), Qt::Key_Right);
        QCOMPARE(hostPage->property("currentIndex").toInt(), 1);
    }

    void uiScalePreservesViewport_data()
    {
        QTest::addColumn<int>("percent");
        QTest::addColumn<QSize>("windowSize");
        QTest::newRow("quarter-4k") << 25 << QSize(3840, 2160);
        QTest::newRow("default-4k") << 100 << QSize(3840, 2160);
        QTest::newRow("fractional-4k") << 125 << QSize(3840, 2160);
        QTest::newRow("double-4k") << 200 << QSize(3840, 2160);
        QTest::newRow("double-deck") << 200 << QSize(1280, 800);
    }

    void uiScalePreservesViewport()
    {
        QFETCH(int, percent);
        QFETCH(QSize, windowSize);
        m_Window->resize(windowSize);
        evaluate(QString("StreamingPreferences.uiScale = %1").arg(percent));
        const qreal factor = percent / 100.0;
        QTRY_COMPARE(evaluate("window.uiScaleFactor").toReal(), factor);
        QTRY_COMPARE(evaluate("uiViewport.width").toReal(), windowSize.width() / factor);
        QTRY_COMPARE(evaluate("uiViewport.height").toReal(), windowSize.height() / factor);

        auto viewport = qobject_cast<QQuickItem*>(evaluate("uiViewport").value<QObject*>());
        auto toolbar = qobject_cast<QQuickItem*>(evaluate("toolBar").value<QObject*>());
        auto stack = qobject_cast<QQuickItem*>(evaluate("stackView").value<QObject*>());
        QVERIFY(viewport && toolbar && stack);
        const auto bottomRight = viewport->mapToScene(QPointF(viewport->width(), viewport->height()));
        QVERIFY(qAbs(bottomRight.x() - windowSize.width()) < 0.01);
        QVERIFY(qAbs(bottomRight.y() - windowSize.height()) < 0.01);
        QVERIFY(qAbs(toolbar->mapToScene(QPointF(0, toolbar->height())).y() - 60 * factor) < 0.01);
        QCOMPARE(stack->mapToScene(QPointF()).y(), toolbar->mapToScene(QPointF(0, toolbar->height())).y());

        // Scaling must trigger the same layout path as a real smaller viewport,
        // including grid centering before any pointer or gamepad input.
        evaluate("ComputerManager.appCount = 40");
        openGames();
        auto games = page();
        QTRY_VERIFY(firstGridDelegate(games));
        QCOMPARE(firstGridDelegate(games)->mapToItem(qobject_cast<QQuickItem*>(games), QPointF()).x(),
                 games->property("leftMargin").toReal());
    }

    void scaledModalRemainsCenteredAfterResize_data()
    {
        QTest::addColumn<int>("percent");
        QTest::newRow("quarter") << 25;
        QTest::newRow("fractional") << 125;
        QTest::newRow("double") << 200;
    }

    void scaledModalRemainsCenteredAfterResize()
    {
        QFETCH(int, percent);
        const qreal factor = percent / 100.0;
        m_Window->resize(1280, 800);
        evaluate(QString("StreamingPreferences.uiScale = %1").arg(percent));
        evaluate("quitConfirmationDialog.open()");
        QTRY_VERIFY(evaluate("quitConfirmationDialog.opened").toBool());
        auto popup = qobject_cast<QQuickItem*>(evaluate("quitConfirmationDialog.contentItem.parent").value<QObject*>());
        QVERIFY(popup);

        for (const auto& size : {QSize(1280, 800), QSize(1600, 1000), QSize(1280, 800)}) {
            m_Window->resize(size);
            QTRY_COMPARE(evaluate("Overlay.overlay.width").toReal(), size.width() / factor);
            QTRY_COMPARE(evaluate("Overlay.overlay.height").toReal(), size.height() / factor);
            QTRY_VERIFY_WITH_TIMEOUT(qAbs(popup->mapToScene(QPointF(popup->width() / 2, popup->height() / 2)).x()
                             - size.width() / 2.0) <= factor, 1000);
            QTRY_VERIFY_WITH_TIMEOUT(qAbs(popup->mapToScene(QPointF(popup->width() / 2, popup->height() / 2)).y()
                             - size.height() / 2.0) <= factor, 1000);
        }

        // Profile changes can also resize the logical overlay while a dialog
        // is open. It must track that geometry without being closed/reopened.
        evaluate(QString("StreamingPreferences.uiScale = %1").arg(percent == 200 ? 100 : 200));
        QTRY_VERIFY(qAbs(popup->mapToScene(QPointF(popup->width() / 2, popup->height() / 2)).x() - 640.0) <= 2.0);
        QTRY_VERIFY(qAbs(popup->mapToScene(QPointF(popup->width() / 2, popup->height() / 2)).y() - 400.0) <= 2.0);

        auto noButton = qobject_cast<QQuickItem*>(evaluate("quitConfirmationDialog.standardButton(Dialog.No)").value<QObject*>());
        QVERIFY(noButton);
        QTest::mouseClick(m_Window.get(), Qt::LeftButton, Qt::NoModifier,
                          noButton->mapToScene(QPointF(noButton->width() / 2, noButton->height() / 2)).toPoint());
        QTRY_VERIFY(!evaluate("quitConfirmationDialog.visible").toBool());
        QCOMPARE(depth(), 2);
    }

    void scaledComboPopupMatchesControl_data()
    {
        QTest::addColumn<int>("percent");
        QTest::newRow("quarter") << 25;
        QTest::newRow("fractional") << 125;
        QTest::newRow("double") << 200;
    }

    void scaledComboPopupMatchesControl()
    {
        QFETCH(int, percent);
        m_Window->resize(1280, 800);
        evaluate(QString("StreamingPreferences.uiScale = %1").arg(percent));
        const qreal factor = percent / 100.0;
        auto combo = qobject_cast<QQuickItem*>(evaluate(
            "Qt.createQmlObject('import QtQuick 2.9; import QtQuick.Controls 2.2; import \"qrc:/gui\"; "
            "AutoResizingComboBox { x: 40; y: 80; width: 180; model: [\"100%\", \"125%\", \"200%\"]; "
            "property int chosen: -1; onActivated: function(index) { chosen = index } }', uiViewport)").value<QObject*>());
        QVERIFY(combo);
        QTest::mouseClick(m_Window.get(), Qt::LeftButton, Qt::NoModifier,
                          combo->mapToScene(QPointF(combo->width() / 2, combo->height() / 2)).toPoint());
        QTRY_VERIFY(evaluate("popup.opened", combo).toBool());
        auto popup = qobject_cast<QQuickItem*>(evaluate("popup.contentItem.parent", combo).value<QObject*>());
        QVERIFY(popup);
        const auto popupOrigin = popup->mapToScene(QPointF());
        QVERIFY(qAbs(popupOrigin.x() - combo->mapToScene(QPointF()).x()) < 1.0);
        const auto popupRight = popup->mapToScene(QPointF(popup->width(), 0));
        QVERIFY(qAbs((popupRight.x() - popupOrigin.x()) - popup->width() * factor) < 0.01);

        auto lastOption = qobject_cast<QQuickItem*>(evaluate(
            "(function() { var items = popup.contentItem.contentItem.children; "
            "for (var i = 0; i < items.length; ++i) if (items[i].text === '200%') return items[i]; "
            "return null; })()", combo).value<QObject*>());
        QVERIFY(lastOption);
        QTest::mouseClick(m_Window.get(), Qt::LeftButton, Qt::NoModifier,
                          lastOption->mapToScene(QPointF(lastOption->width() / 2, lastOption->height() / 2)).toPoint());
        QTRY_COMPARE(combo->property("chosen").toInt(), 2);
        QTRY_VERIFY(!evaluate("popup.visible", combo).toBool());
    }

    void uiScaleUpdatesWithoutRecreatingPage()
    {
        m_Window->resize(1280, 800);
        auto originalPage = page();
        for (const int percent : {200, 125, 25, 100}) {
            evaluate(QString("StreamingPreferences.uiScale = %1").arg(percent));
            QTRY_COMPARE(evaluate("window.uiScaleFactor").toReal(), percent / 100.0);
            QCOMPARE(page(), originalPage);
            QCOMPARE(depth(), 2);
        }

        evaluate("ProfileManager.hasActiveProfile = false");
        evaluate("StreamingPreferences.uiScale = 200");
        QTRY_COMPARE(evaluate("window.uiScaleFactor").toReal(), 1.0);
        evaluate("ProfileManager.hasActiveProfile = true");
        QTRY_COMPARE(evaluate("window.uiScaleFactor").toReal(), 2.0);
    }

    void scaledContextMenuRemainsInsideWindow()
    {
        m_Window->resize(1280, 800);
        evaluate("StreamingPreferences.uiScale = 200");
        auto menu = evaluate(
            "Qt.createQmlObject('import QtQuick 2.9; import QtQuick.Controls 2.5; "
            "Menu { x: 580; y: 360; property bool chosen: false; "
            "MenuItem { text: \"Scale test\"; onTriggered: chosen = true } }', uiViewport)").value<QObject*>();
        QVERIFY(menu);
        evaluate("open()", menu);
        QTRY_VERIFY(evaluate("opened", menu).toBool());
        auto popup = qobject_cast<QQuickItem*>(evaluate("contentItem.parent", menu).value<QObject*>());
        QVERIFY(popup);
        auto item = qobject_cast<QQuickItem*>(evaluate("itemAt(0)", menu).value<QObject*>());
        QVERIFY(item);
        // A context menu near the lower/right edge must constrain against the
        // logical overlay bounds, not expand past the physical window edge.
        const auto topLeft = popup->mapToScene(QPointF());
        const auto bottomRight = popup->mapToScene(QPointF(popup->width(), popup->height()));
        QVERIFY(topLeft.x() >= 0 && topLeft.y() >= 0);
        QVERIFY(bottomRight.x() <= m_Window->width() + 1);
        QVERIFY(bottomRight.y() <= m_Window->height() + 1);
        QVERIFY(qAbs((bottomRight.x() - topLeft.x()) - popup->width() * 2.0) < 0.01);
        QTest::mouseClick(m_Window.get(), Qt::LeftButton, Qt::NoModifier,
                          item->mapToScene(QPointF(item->width() / 2, item->height() / 2)).toPoint());
        QTRY_VERIFY(menu->property("chosen").toBool());
        QTRY_VERIFY(!menu->property("visible").toBool());
    }

    void gamesAreAlignedBeforeInput_data()
    {
        QTest::addColumn<int>("appCount");
        QTest::addColumn<int>("windowWidth");
        QTest::addColumn<int>("gamepads");
        QTest::newRow("single-game") << 1 << 1280 << 0;
        QTest::newRow("many-games-mouse") << 40 << 1280 << 0;
        QTest::newRow("many-games-controller") << 40 << 1280 << 1;
        QTest::newRow("narrow-window") << 40 << 800 << 0;
    }

    void gamesAreAlignedBeforeInput()
    {
        QFETCH(int, appCount);
        QFETCH(int, windowWidth);
        QFETCH(int, gamepads);
        m_Window->setWidth(windowWidth);
        evaluate(QString("ComputerManager.appCount = %1").arg(appCount));
        evaluate(QString("SdlGamepadKeyNavigation.connectedGamepads = %1").arg(gamepads));
        openGames();
        auto games = page();
        QCOMPARE(games->property("count").toInt(), appCount);
        QCOMPARE(games->property("currentIndex").toInt(), gamepads ? 0 : -1);
        QTRY_VERIFY(firstGridDelegate(games));
        QCOMPARE(firstGridDelegate(games)->mapToItem(qobject_cast<QQuickItem*>(games), QPointF()).x(),
                 games->property("leftMargin").toReal());
    }

    void gamesStayAlignedWhenLoadedAfterOpening()
    {
        evaluate("ComputerManager.appCount = 0");
        openGames();
        auto games = page();
        evaluate("appModel.populate(40)", games);
        QTRY_COMPARE(games->property("count").toInt(), 40);
        QTRY_VERIFY(firstGridDelegate(games));
        QCOMPARE(firstGridDelegate(games)->mapToItem(qobject_cast<QQuickItem*>(games), QPointF()).x(),
                 games->property("leftMargin").toReal());
    }

    void gridRecenteringPreservesVerticalScroll()
    {
        evaluate("ComputerManager.appCount = 80");
        openGames();
        auto games = page();
        evaluate("positionViewAtIndex(count - 1, GridView.End)", games);
        const auto scrollY = games->property("contentY").toReal();
        QVERIFY(scrollY > 0);
        const auto margin = games->property("leftMargin").toReal();

        // Keep the same number of columns so only the horizontal margins change.
        m_Window->setWidth(m_Window->width() + 40);
        QTRY_VERIFY(games->property("leftMargin").toReal() != margin);
        QCOMPARE(games->property("contentX").toReal(), -games->property("leftMargin").toReal());
        QCOMPARE(games->property("contentY").toReal(), scrollY);

        evaluate("stackView.push(Qt.createQmlObject('import QtQuick 2.9; Item {}', stackView))");
        QTRY_VERIFY(!busy());
        evaluate("stackView.pop()");
        QTRY_VERIFY(!busy());
        QCOMPARE(page(), games);
        QCOMPARE(games->property("contentX").toReal(), -games->property("leftMargin").toReal());
        QCOMPARE(games->property("contentY").toReal(), scrollY);
    }

    void rapidBack_data()
    {
        QTest::addColumn<int>("key");
        QTest::newRow("controller-B") << int(Qt::Key_Escape);
        QTest::newRow("platform-back") << int(Qt::Key_Back);
    }

    void rapidBack()
    {
        QFETCH(int, key);
        openGames();
        for (int i = 0; i < 12; i++) {
            QTest::keyClick(m_Window.get(), Qt::Key(key));
        }
        QTRY_VERIFY(!busy());
        QCOMPARE(depth(), 2);
        QCOMPARE(page()->objectName(), QString("Computers"));
        QTest::keyClick(m_Window.get(), Qt::Key_Right);
        QCOMPARE(page()->property("currentIndex").toInt(), 1);

        QTest::keyClick(m_Window.get(), Qt::Key(key));
        QTRY_VERIFY(!busy());
        QCOMPARE(depth(), 1);
        QCOMPARE(page()->objectName(), QString("Profiles"));
        QTest::keyClick(m_Window.get(), Qt::Key_Right);
        QTest::keyClick(m_Window.get(), Qt::Key_Return);
        QTRY_VERIFY(!busy());
        QCOMPARE(depth(), 2);
        QCOMPARE(evaluate("ProfileManager.activeProfileId").toString(), QString("second"));
    }

    void hiddenProfileCannotStealFocusOrActivate()
    {
        openGames();
        auto profilePage = evaluate("stackView.get(0)").value<QObject*>();
        QVERIFY(profilePage);
        evaluate("profileRoot.restoreGridFocusSoon()", profilePage);
        QTest::qWait(40);
        QVERIFY(qobject_cast<QQuickItem*>(page())->hasActiveFocus());
        evaluate("profileRoot.activateProfile('second')", profilePage);
        QCOMPARE(depth(), 3);
        QCOMPARE(evaluate("ProfileManager.activeProfileId").toString(), QString("default"));
    }

    void staleComputerLostCannotPopAnotherPage()
    {
        openGames();
        QPointer<QObject> gamesPage = page();
        QPointer<QObject> gamesModel = evaluate("appModel", gamesPage).value<QObject*>();
        QTest::keyClick(m_Window.get(), Qt::Key_Escape);
        evaluate("computerLost()", gamesPage);
        QTRY_VERIFY(!busy());
        QCOMPARE(depth(), 2);
        QTRY_VERIFY(gamesPage.isNull());
        QTRY_VERIFY(gamesModel.isNull());
    }

    void navigationDuringTransitionIsIgnored()
    {
        openGames();
        QTest::keyClick(m_Window.get(), Qt::Key_Escape);
        QVERIFY(busy());
        evaluate("window.returnToProfileSelection()");
        QVERIFY(!evaluate("window.enterProfile('second', true)").toBool());
        evaluate("window.navigateTo('qrc:/gui/SettingsView.qml', SettingsView)");
        evaluate("stackView.currentItem.openComputer(0, 'Test PC', false)");
        QTRY_VERIFY(!busy());
        QCOMPARE(depth(), 2);
        QCOMPARE(evaluate("ProfileManager.activeProfileId").toString(), QString("default"));
    }

    void dismissQuitDialogRestoresProfileFocus()
    {
        evaluate("window.returnToProfileSelection()");
        QTRY_VERIFY(!busy());
        QTest::keyClick(m_Window.get(), Qt::Key_Escape);
        QTRY_VERIFY(evaluate("quitConfirmationDialog.opened").toBool());
        QTest::keyClick(m_Window.get(), Qt::Key_Escape);
        QTRY_VERIFY(!evaluate("quitConfirmationDialog.visible").toBool());
        QTest::keyClick(m_Window.get(), Qt::Key_Right);
        QTest::keyClick(m_Window.get(), Qt::Key_Return);
        QTRY_VERIFY(!busy());
        QCOMPARE(depth(), 2);
        QCOMPARE(evaluate("ProfileManager.activeProfileId").toString(), QString("second"));
    }

    void profileModalBackOnlyClosesTopDialog_data()
    {
        QTest::addColumn<int>("key");
        QTest::newRow("controller-B") << int(Qt::Key_Escape);
        QTest::newRow("platform-back") << int(Qt::Key_Back);
        QTest::newRow("toolbar-back") << 0;
    }

    void profileModalBackOnlyClosesTopDialog()
    {
        QFETCH(int, key);
        evaluate("window.returnToProfileSelection()");
        QTRY_VERIFY(!busy());
        auto profiles = page();
        evaluate("editProfileDialog.profileId = 'second'; editProfileDialog.profileName = 'Second'; editProfileDialog.open()", profiles);
        QTRY_VERIFY(evaluate("editProfileDialog.opened", profiles).toBool());
        evaluate("errorDialog.text = 'Test profile error'; errorDialog.open()", profiles);
        QTRY_VERIFY(evaluate("errorDialog.opened", profiles).toBool());

        // The profile page has its own delayed focus restoration in addition to
        // main.qml's current-view handler. Neither may steal a modal's focus.
        evaluate("profileRoot.focusCurrentGridItem(); profileRoot.restoreGridFocusSoon()", profiles);
        QTest::qWait(40);
        QVERIFY(evaluate("errorDialog.standardButton(Dialog.Ok).activeFocus", profiles).toBool());
        if (key) QTest::keyClick(m_Window.get(), Qt::Key(key));
        else evaluate("window.goBack()");
        QTRY_VERIFY(!evaluate("errorDialog.visible", profiles).toBool());
        QVERIFY(evaluate("editProfileDialog.visible", profiles).toBool());
        QCOMPARE(depth(), 1);
        QVERIFY(!evaluate("quitConfirmationDialog.visible").toBool());

        if (key == Qt::Key_Back) {
            QKeyEvent repeat(QEvent::KeyPress, Qt::Key_Back, Qt::NoModifier, QString(), true);
            QCoreApplication::sendEvent(m_Window.get(), &repeat);
            QVERIFY(evaluate("editProfileDialog.visible", profiles).toBool());
            QCOMPARE(depth(), 1);
        }

        if (key) QTest::keyClick(m_Window.get(), Qt::Key(key));
        else evaluate("window.goBack()");
        QTRY_VERIFY(!evaluate("editProfileDialog.visible", profiles).toBool());
        QCOMPARE(page(), profiles);
        QVERIFY(!evaluate("quitConfirmationDialog.visible").toBool());
        QTRY_VERIFY(evaluate("profileGrid.activeFocus", profiles).toBool());
        QTest::keyClick(m_Window.get(), Qt::Key_Right);
        QCOMPARE(evaluate("profileGrid.currentIndex", profiles).toInt(), 1);
    }

    void profileSwitchBehindDialogKeepsModalFocus()
    {
        evaluate("quitConfirmationDialog.open()");
        QTRY_VERIFY(evaluate("quitConfirmationDialog.opened").toBool());
        QVERIFY(evaluate("window.enterProfile('second', true, true)").toBool());
        QTRY_VERIFY(!busy());
        QCOMPARE(depth(), 2);
        QCOMPARE(evaluate("ProfileManager.activeProfileId").toString(), QString("second"));
        QVERIFY(evaluate("quitConfirmationDialog.activeFocus").toBool());
        QTest::keyClick(m_Window.get(), Qt::Key_Escape);
        QTRY_VERIFY(!evaluate("quitConfirmationDialog.visible").toBool());
        QTRY_VERIFY(qobject_cast<QQuickItem*>(page())->hasActiveFocus());
        QTest::keyClick(m_Window.get(), Qt::Key_Right);
        QCOMPARE(page()->property("currentIndex").toInt(), 1);
    }

    void defaultHostDoesNotReopenOnBack()
    {
        evaluate("ComputerManager.defaultHostUuid = 'pc-1'");
        evaluate("window.returnToProfileSelection()");
        QTRY_VERIFY(!busy());
        evaluate("window.enterProfile('default', true, true)");
        QTRY_COMPARE(depth(), 3);
        QTRY_VERIFY(!busy());
        QTest::keyClick(m_Window.get(), Qt::Key_Escape);
        QTRY_VERIFY(!busy());
        QTest::qWait(40);
        QCOMPARE(depth(), 2);
        QVERIFY(qobject_cast<QQuickItem*>(page())->hasActiveFocus());
    }

    void defaultHostWaitsForStatus_data()
    {
        QTest::addColumn<bool>("selectAnotherHost");
        QTest::newRow("default-host") << false;
        QTest::newRow("user-selected-another-host") << true;
    }

    void defaultHostWaitsForStatus()
    {
        QFETCH(bool, selectAnotherHost);
        evaluate("window.returnToProfileSelection()");
        QTRY_VERIFY(!busy());
        evaluate("ComputerManager.defaultHostUuid = 'pc-1'");
        evaluate("window.enterProfile('default', true, true)");
        auto hostPage = page();
        evaluate("computerModel.setProperty(0, 'statusUnknown', true)", hostPage);
        QTRY_VERIFY(!busy());
        QCOMPARE(depth(), 2);

        if (selectAnotherHost) {
            QTest::keyClick(m_Window.get(), Qt::Key_Right);
            QCOMPARE(hostPage->property("currentIndex").toInt(), 1);
        }
        evaluate("computerModel.setProperty(0, 'statusUnknown', false)", hostPage);

        if (selectAnotherHost) {
            QCOMPARE(hostPage->property("currentIndex").toInt(), 1);
            QTest::qWait(40);
            QCOMPARE(depth(), 2);
        }
        else {
            QTRY_COMPARE(depth(), 3);
            QTRY_VERIFY(!busy());
        }
    }

};

QTEST_MAIN(NavigationTest)
#include "tst_navigation.moc"
