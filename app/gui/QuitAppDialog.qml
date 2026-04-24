import QtQuick 2.0
import QtQuick.Controls 2.2

NavigableMessageDialog {
    property var appModel
    property var stackViewRef
    property string appName: ""
    property bool segueToStream: false
    property string nextAppName: ""
    property int nextAppIndex: 0

    text: qsTr("Are you sure you want to quit %1? Any unsaved progress will be lost.").arg(appName)
    standardButtons: Dialog.Yes | Dialog.No

    function quitApp() {
        var component = Qt.createComponent("QuitSegue.qml")
        var params = {"appName": appName, "quitRunningAppFn": function() { appModel.quitRunningApp() }}
        if (segueToStream) {
            params.nextAppName = nextAppName
            params.nextSession = appModel.createSessionForApp(nextAppIndex)
        }
        else {
            params.nextAppName = null
            params.nextSession = null
        }

        stackViewRef.push(component.createObject(stackViewRef, params))
    }

    onAccepted: quitApp()
}
