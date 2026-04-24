import QtQuick 2.9
import QtQuick.Controls 2.2
import QtQuick.Controls.Material 2.2

import StreamingPreferences 1.0

NavigableItemDelegate {
    property var appModel
    property var quitAppDialog
    property var stackViewRef

    id: delegateRoot
    width: 220
    height: 287
    grid: GridView.view

    property alias appContextMenu: appContextMenuLoader.item
    property alias appNameText: appNameTextLoader.item

    opacity: model.hidden ? 0.4 : 1.0

    Image {
        property bool isPlaceholder: false

        id: appIcon
        anchors.horizontalCenter: parent.horizontalCenter
        y: 10
        source: model.boxart

        onSourceSizeChanged: {
            if (!model.isAppCollectorGame &&
                ((sourceSize.width === 130 && sourceSize.height === 180) ||
                 (sourceSize.width === 628 && sourceSize.height === 888) ||
                 (sourceSize.width === 200 && sourceSize.height === 266)))
            {
                isPlaceholder = true
            }
            else
            {
                isPlaceholder = false
            }

            width = 200
            height = 267
        }

        ToolTip.text: model.name
        ToolTip.delay: 1000
        ToolTip.timeout: 5000
        ToolTip.visible: (parent.hovered || parent.highlighted) && (!appNameText || appNameText.truncated)
    }

    Loader {
        active: model.running
        asynchronous: true
        anchors.fill: appIcon

        sourceComponent: Item {
            RoundButton {
                focusPolicy: Qt.NoFocus

                anchors.horizontalCenterOffset: appIcon.isPlaceholder ? -47 : 0
                anchors.verticalCenterOffset: appIcon.isPlaceholder ? -75 : -60
                anchors.centerIn: parent
                implicitWidth: 85
                implicitHeight: 85

                icon.source: "qrc:/res/play_arrow_FILL1_wght700_GRAD200_opsz48.svg"
                icon.width: 75
                icon.height: 75

                onClicked: {
                    launchOrResumeSelectedApp(true)
                }

                ToolTip.text: qsTr("Resume Game")
                ToolTip.delay: 1000
                ToolTip.timeout: 3000
                ToolTip.visible: hovered

                Material.background: StreamingPreferences.theme === StreamingPreferences.THEME_OLED ? "#D0000000" : "#D0808080"
            }

            RoundButton {
                focusPolicy: Qt.NoFocus

                anchors.horizontalCenterOffset: appIcon.isPlaceholder ? 47 : 0
                anchors.verticalCenterOffset: appIcon.isPlaceholder ? -75 : 60
                anchors.centerIn: parent
                implicitWidth: 85
                implicitHeight: 85

                icon.source: "qrc:/res/stop_FILL1_wght700_GRAD200_opsz48.svg"
                icon.width: 75
                icon.height: 75

                onClicked: {
                    doQuitGame()
                }

                ToolTip.text: qsTr("Quit Game")
                ToolTip.delay: 1000
                ToolTip.timeout: 3000
                ToolTip.visible: hovered

                Material.background: StreamingPreferences.theme === StreamingPreferences.THEME_OLED ? "#D0000000" : "#D0808080"
            }
        }
    }

    Loader {
        id: appNameTextLoader
        active: appIcon.isPlaceholder

        width: appIcon.width
        height: model.running ? 175 : appIcon.height

        anchors.left: appIcon.left
        anchors.right: appIcon.right
        anchors.bottom: appIcon.bottom

        sourceComponent: Label {
            id: appNameText
            text: model.name
            font.pointSize: 22
            leftPadding: 20
            rightPadding: 20
            verticalAlignment: Text.AlignVCenter
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            elide: Text.ElideRight
        }
    }

    function launchOrResumeSelectedApp(quitExistingApp)
    {
        var runningId = appModel.getRunningAppId()
        if (runningId !== 0 && runningId !== model.appid) {
            if (quitExistingApp) {
                quitAppDialog.appName = appModel.getRunningAppName()
                quitAppDialog.segueToStream = true
                quitAppDialog.nextAppName = model.name
                quitAppDialog.nextAppIndex = index
                quitAppDialog.open()
            }

            return
        }

        var component = Qt.createComponent("StreamSegue.qml")
        var segue = component.createObject(stackViewRef, {
                                               "appName": model.name,
                                               "session": appModel.createSessionForApp(index),
                                               "isResume": runningId === model.appid
                                           })
        stackViewRef.push(segue)
    }

    onClicked: {
        if (!model.running) {
            launchOrResumeSelectedApp(true)
        }
    }

    onPressAndHold: {
        if (appContextMenu.popup) {
            appContextMenu.popup()
        }
        else {
            appContextMenu.open()
        }
    }

    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.RightButton
        onClicked: parent.pressAndHold()
    }

    Keys.onReturnPressed: {
        if (model.running) {
            appContextMenu.open()
        }
    }

    Keys.onEnterPressed: {
        if (model.running) {
            appContextMenu.open()
        }
    }

    Keys.onMenuPressed: {
        appContextMenu.open()
    }

    function doQuitGame() {
        quitAppDialog.appName = appModel.getRunningAppName()
        quitAppDialog.segueToStream = false
        quitAppDialog.open()
    }

    Loader {
        id: appContextMenuLoader
        asynchronous: true
        sourceComponent: NavigableMenu {
            id: appContextMenu
            initiator: delegateRoot

            NavigableMenuItem {
                text: model.running ? qsTr("Resume Game") : qsTr("Launch Game")
                onTriggered: launchOrResumeSelectedApp(true)
            }

            NavigableMenuItem {
                text: qsTr("Quit Game")
                visible: model.running
                onTriggered: doQuitGame()
            }

            NavigableMenuItem {
                checkable: true
                checked: model.directLaunch
                text: qsTr("Direct Launch")
                enabled: !model.hidden
                onTriggered: appModel.setAppDirectLaunch(model.index, !model.directLaunch)

                ToolTip.text: qsTr("Launch this app immediately when the host is selected, bypassing the app selection grid.")
                ToolTip.delay: 1000
                ToolTip.timeout: 3000
                ToolTip.visible: hovered
            }

            NavigableMenuItem {
                checkable: true
                checked: model.hidden
                text: qsTr("Hide Game")
                enabled: model.hidden || (!model.running && !model.directLaunch)
                onTriggered: appModel.setAppHidden(model.index, !model.hidden)

                ToolTip.text: qsTr("Hide this game from the app grid. To access hidden games, right-click on the host and choose %1.").arg(qsTr("View All Apps"))
                ToolTip.delay: 1000
                ToolTip.timeout: 5000
                ToolTip.visible: hovered
            }
        }
    }
}
