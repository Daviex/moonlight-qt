import QtQuick 2.0
import QtQuick.Controls 2.2

import ComputerManager 1.0

Item {
    property bool launcherSignalsConnected : false

    function onSearchingComputer() {
        stageLabel.text = qsTr("Establishing connection to PC...")
    }

    function onSearchingApp() {
        stageLabel.text = qsTr("Loading app list...")
    }

    function onSessionCreated(appName, session) {
        disconnectLauncherSignals()

        var component = Qt.createComponent("StreamSegue.qml")
        var segue = component.createObject(stackView, {
            "appName": appName,
            "session": session,
            "quitAfter": true
        })
        stackView.push(segue)
    }

    function onLaunchFailed(message) {
        disconnectLauncherSignals()

        errorDialog.text = message
        errorDialog.open()
        console.error(message)
    }

    function onAppQuitRequired(appName) {
        quitAppDialog.appName = appName
        quitAppDialog.open()
    }

    function connectLauncherSignals() {
        if (launcherSignalsConnected) {
            return
        }

        launcher.searchingComputer.connect(onSearchingComputer)
        launcher.searchingApp.connect(onSearchingApp)
        launcher.sessionCreated.connect(onSessionCreated)
        launcher.failed.connect(onLaunchFailed)
        launcher.appQuitRequired.connect(onAppQuitRequired)
        launcherSignalsConnected = true
    }

    function disconnectLauncherSignals() {
        if (!launcherSignalsConnected) {
            return
        }

        launcher.searchingComputer.disconnect(onSearchingComputer)
        launcher.searchingApp.disconnect(onSearchingApp)
        launcher.sessionCreated.disconnect(onSessionCreated)
        launcher.failed.disconnect(onLaunchFailed)
        launcher.appQuitRequired.disconnect(onAppQuitRequired)
        launcherSignalsConnected = false
    }

    StackView.onActivated: {
        if (!launcher.isExecuted()) {
            toolBar.visible = false

            connectLauncherSignals()
            launcher.execute(ComputerManager)
        }
    }

    Component.onDestruction: {
        disconnectLauncherSignals()
    }

    Row {
        anchors.centerIn: parent
        spacing: 5

        BusyIndicator {
            id: stageSpinner
            running: visible
        }

        Label {
            id: stageLabel
            height: stageSpinner.height
            font.pointSize: 20
            verticalAlignment: Text.AlignVCenter

            wrapMode: Text.Wrap
        }
    }

    ErrorMessageDialog {
        id: errorDialog

        onClosed: {
            Qt.quit();
        }
    }

    NavigableMessageDialog {
        id: quitAppDialog
        text:qsTr("Are you sure you want to quit %1? Any unsaved progress will be lost.").arg(appName)
        standardButtons: Dialog.Yes | Dialog.No
        property string appName : ""

        function quitApp() {
            var component = Qt.createComponent("QuitSegue.qml")
            var params = {"appName": appName, "quitRunningAppFn": function() { launcher.quitRunningApp() }}
            stackView.push(component.createObject(stackView, params))
        }

        onAccepted: quitApp()
        onRejected: Qt.quit()
    }
}
