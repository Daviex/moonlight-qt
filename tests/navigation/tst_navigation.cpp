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
