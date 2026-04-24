import QtQuick 2.9
import QtQuick.Controls 2.2

NavigableItemDelegate {
    property var computerModel
    property var errorDialog
    property var pairDialog
    property var testConnectionDialog
    property var renamePcDialog
    property var deletePcDialog
    property var showPcDetailsDialog
    property var stackViewRef

    id: delegateRoot
    width: 300
    height: 320
    grid: GridView.view

    property alias pcContextMenu: pcContextMenuLoader.item

    Image {
        id: pcIcon
        anchors.horizontalCenter: parent.horizontalCenter
        source: "qrc:/res/desktop_windows-48px.svg"
        sourceSize {
            width: 200
            height: 200
        }
    }

    Image {
        // TODO: Tooltip
        id: stateIcon
        anchors.horizontalCenter: pcIcon.horizontalCenter
        anchors.verticalCenter: pcIcon.verticalCenter
        anchors.verticalCenterOffset: !model.online ? -18 : -16
        visible: !model.statusUnknown && (!model.online || !model.paired)
        source: !model.online ? "qrc:/res/warning_FILL1_wght300_GRAD200_opsz24.svg" : "qrc:/res/baseline-lock-24px.svg"
        sourceSize {
            width: !model.online ? 75 : 70
            height: !model.online ? 75 : 70
        }
    }

    BusyIndicator {
        id: statusUnknownSpinner
        anchors.horizontalCenter: pcIcon.horizontalCenter
        anchors.verticalCenter: pcIcon.verticalCenter
        anchors.verticalCenterOffset: -15
        width: 75
        height: 75
        visible: model.statusUnknown
        running: visible
    }

    Label {
        id: pcNameText
        text: model.name

        width: parent.width
        anchors.top: pcIcon.bottom
        anchors.bottom: parent.bottom
        font.pointSize: 36
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        elide: Text.ElideRight
    }

    Loader {
        id: pcContextMenuLoader
        asynchronous: true
        sourceComponent: NavigableMenu {
            id: pcContextMenu
            initiator: delegateRoot

            MenuItem {
                text: qsTr("PC Status: %1").arg(model.online ? qsTr("Online") : qsTr("Offline"))
                font.bold: true
                enabled: false
            }

            NavigableMenuItem {
                text: qsTr("View All Apps")
                visible: model.online && model.paired

                onTriggered: {
                    var component = Qt.createComponent("AppView.qml")
                    var appView = component.createObject(stackViewRef, {
                                                           "computerIndex": index,
                                                           "objectName": model.name,
                                                           "showHiddenGames": true
                                                       })
                    stackViewRef.push(appView)
                }
            }

            NavigableMenuItem {
                text: qsTr("Wake PC")
                visible: !model.online && model.wakeable
                onTriggered: computerModel.wakeComputer(index)
            }

            NavigableMenuItem {
                text: qsTr("Test Network")
                onTriggered: {
                    computerModel.testConnectionForComputer(index)
                    testConnectionDialog.open()
                }
            }

            NavigableMenuItem {
                text: qsTr("Rename PC")
                onTriggered: {
                    renamePcDialog.pcIndex = index
                    renamePcDialog.originalName = model.name
                    renamePcDialog.open()
                }
            }

            NavigableMenuItem {
                text: qsTr("Delete PC")
                onTriggered: {
                    deletePcDialog.pcIndex = index
                    deletePcDialog.pcName = model.name
                    deletePcDialog.open()
                }
            }

            NavigableMenuItem {
                text: qsTr("View Details")
                onTriggered: {
                    showPcDetailsDialog.pcDetails = model.details
                    showPcDetailsDialog.open()
                }
            }
        }
    }

    onClicked: {
        if (model.online) {
            if (!model.serverSupported) {
                errorDialog.text = qsTr("The version of GeForce Experience on %1 is not supported by this build of Moonlight. You must update Moonlight to stream from %1.").arg(model.name)
                errorDialog.helpText = ""
                errorDialog.open()
            }
            else if (model.paired) {
                var component = Qt.createComponent("AppView.qml")
                var appView = component.createObject(stackViewRef, {
                                                       "computerIndex": index,
                                                       "objectName": model.name
                                                   })
                stackViewRef.push(appView)
            }
            else {
                var pin = computerModel.generatePinString()

                computerModel.pairComputer(index, pin)

                pairDialog.pin = pin
                pairDialog.open()
            }
        }
        else {
            pcContextMenu.open()
        }
    }

    onPressAndHold: {
        if (pcContextMenu.popup) {
            pcContextMenu.popup()
        }
        else {
            pcContextMenu.open()
        }
    }

    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.RightButton
        onClicked: parent.pressAndHold()
    }

    Keys.onMenuPressed: {
        pcContextMenu.open()
    }

    Keys.onDeletePressed: {
        deletePcDialog.pcIndex = index
        deletePcDialog.pcName = model.name
        deletePcDialog.open()
    }
}
