import QtQuick 2.9
import QtQuick.Controls 2.2
import AppModel 1.0
import ComputerManager 1.0
import SdlGamepadKeyNavigation 1.0

CenteredGridView {
    property int computerIndex
    property AppModel appModel : createModel()
    property bool activated
    property bool showHiddenGames
    property bool showGames

    id: appGrid
    focus: true
    activeFocusOnTab: true
    topMargin: 20
    bottomMargin: 5
    cellWidth: 230; cellHeight: 297;

    function computerLost()
    {
        // Go back to the PC view on PC loss
        stackView.pop()
    }

    Component.onCompleted: {
        // Don't show any highlighted item until interacting with them.
        // We do this here instead of onActivated to avoid losing the user's
        // selection when backing out of a different page of the app.
        currentIndex = -1
    }

    StackView.onActivated: {
        appModel.computerLost.connect(computerLost)
        activated = true

        // Highlight the first item if a gamepad is connected
        if (currentIndex === -1 && SdlGamepadKeyNavigation.getConnectedGamepads() > 0) {
            currentIndex = 0
        }

        if (!showGames && !showHiddenGames) {
            // Check if there's a direct launch app
            var directLaunchAppIndex = model.getDirectLaunchAppIndex();
            if (directLaunchAppIndex >= 0) {
                // Start the direct launch app if nothing else is running
                currentIndex = directLaunchAppIndex
                currentItem.launchOrResumeSelectedApp(false)

                // Set showGames so we will not loop when the stream ends
                showGames = true
            }
        }
    }

    StackView.onDeactivating: {
        appModel.computerLost.disconnect(computerLost)
        activated = false
    }

    function createModel()
    {
        var model = Qt.createQmlObject('import AppModel 1.0; AppModel {}', parent, '')
        model.initialize(ComputerManager, computerIndex, showHiddenGames)
        return model
    }

    model: appModel

    delegate: AppGridDelegate {
        appModel: appGrid.appModel
        quitAppDialog: quitAppDialog
        stackViewRef: stackView
    }

    Row {
        anchors.centerIn: parent
        spacing: 5
        visible: appGrid.count === 0

        Label {
            text: qsTr("This computer doesn't seem to have any applications or some applications are hidden")
            font.pointSize: 20
            verticalAlignment: Text.AlignVCenter
            wrapMode: Text.Wrap
        }
    }

    QuitAppDialog {
        id: quitAppDialog
        appModel: appGrid.appModel
        stackViewRef: stackView
    }

    ScrollBar.vertical: ScrollBar {}
}
