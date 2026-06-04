import QtQuick 2.9
import QtQuick.Controls 2.2
import QtQuick.Layouts 1.3
import QtQuick.Controls.Material 2.2

import ProfileManager 1.0
import StreamingPreferences 1.0
import SdlGamepadKeyNavigation 1.0

Item {
    id: profileRoot

    property bool suppressPolling: true
    property bool allowActivation: true

    objectName: allowActivation ? qsTr("Profiles") : qsTr("Manage Profiles")
    focus: true
    activeFocusOnTab: true

    function findProfile(profileId) {
        var profiles = ProfileManager.profiles
        for (var i = 0; i < profiles.length; i++) {
            if (profiles[i].id === profileId) {
                return profiles[i]
            }
        }
        return null
    }

    function currentProfile() {
        if (profileGrid.currentIndex < 0 || profileGrid.currentIndex >= ProfileManager.profiles.length) {
            return null
        }
        return ProfileManager.profiles[profileGrid.currentIndex]
    }

    function activateProfile(profileId) {
        if (!allowActivation) {
            return
        }

        if (!ProfileManager.activateProfile(profileId)) {
            errorDialog.text = qsTr("Unable to activate this profile.")
            errorDialog.open()
            return
        }

        StreamingPreferences.retranslate()
        stackView.replace("qrc:/gui/PcView.qml")
        window.runConfigurationChecks()
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 24
        spacing: 18

        Label {
            text: profileRoot.allowActivation ? qsTr("Choose a profile") : qsTr("Manage profiles")
            font.pointSize: 30
            horizontalAlignment: Text.AlignHCenter
            Layout.fillWidth: true
        }

        CenteredGridView {
            id: profileGrid

            Layout.fillWidth: true
            Layout.fillHeight: true
            cellWidth: 260
            cellHeight: 280
            minMargin: 20
            focus: true
            activeFocusOnTab: true

            model: ProfileManager.profiles

            Component.onCompleted: {
                currentIndex = ProfileManager.profiles.length > 0 ? 0 : -1
                if (currentIndex >= 0 && SdlGamepadKeyNavigation.getConnectedGamepads() > 0) {
                    currentItem.forceActiveFocus(Qt.TabFocus)
                }
            }

            delegate: NavigableItemDelegate {
                id: profileDelegate

                property var profile: modelData
                property alias profileContextMenu: profileContextMenuLoader.item

                width: 250
                height: 260
                grid: profileGrid

                Rectangle {
                    id: avatar
                    width: 140
                    height: 140
                    radius: 70
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.top: parent.top
                    anchors.topMargin: 16
                    color: Material.accent

                    Label {
                        anchors.centerIn: parent
                        text: profile.name.length > 0 ? profile.name.charAt(0).toUpperCase() : "?"
                        font.pointSize: 54
                        font.bold: true
                        color: "white"
                    }
                }

                Label {
                    id: profileNameLabel
                    text: profile.name
                    width: parent.width
                    anchors.top: avatar.bottom
                    anchors.topMargin: 16
                    horizontalAlignment: Text.AlignHCenter
                    font.pointSize: 24
                    elide: Text.ElideRight
                }

                Label {
                    text: profile.defaultProfile ? qsTr("Default") :
                          profile.autoLogin ? qsTr("Auto-login") : ""
                    visible: text.length > 0
                    anchors.top: profileNameLabel.bottom
                    anchors.topMargin: 6
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    font.pointSize: 12
                }

                Loader {
                    id: profileContextMenuLoader
                    sourceComponent: NavigableMenu {
                        initiator: profileContextMenuLoader.parent

                        NavigableMenuItem {
                            text: qsTr("Open Profile")
                            visible: profileRoot.allowActivation
                            onTriggered: activateProfile(profile.id)
                        }

                        NavigableMenuItem {
                            text: qsTr("Rename Profile")
                            onTriggered: {
                                editProfileDialog.profileId = profile.id
                                editProfileDialog.profileName = profile.name
                                editProfileDialog.open()
                            }
                        }

                        NavigableMenuItem {
                            text: qsTr("Set as Default")
                            visible: !profile.defaultProfile
                            onTriggered: ProfileManager.setDefaultProfile(profile.id)
                        }

                        NavigableMenuItem {
                            text: profile.autoLogin ? qsTr("Disable Auto-login") : qsTr("Enable Auto-login")
                            onTriggered: ProfileManager.setAutoLoginProfile(profile.id, !profile.autoLogin)
                        }

                        NavigableMenuItem {
                            text: qsTr("Delete Profile")
                            visible: ProfileManager.profiles.length > 1 &&
                                     (profileRoot.allowActivation || profile.id !== ProfileManager.activeProfileId)
                            onTriggered: {
                                deleteProfileDialog.profileId = profile.id
                                deleteProfileDialog.profileName = profile.name
                                deleteProfileDialog.open()
                            }
                        }
                    }
                }

                onClicked: {
                    if (profileRoot.allowActivation) {
                        activateProfile(profile.id)
                    }
                    else {
                        profileContextMenu.open()
                    }
                }

                onPressAndHold: {
                    if (profileContextMenu.popup) {
                        profileContextMenu.popup()
                    }
                    else {
                        profileContextMenu.open()
                    }
                }
            }
        }

        RowLayout {
            Layout.alignment: Qt.AlignHCenter
            spacing: 12

            Button {
                text: qsTr("Add")
                onClicked: {
                    editProfileDialog.profileId = ""
                    editProfileDialog.profileName = ""
                    editProfileDialog.open()
                }
            }

            Button {
                text: qsTr("Edit")
                enabled: profileRoot.currentProfile() !== null
                onClicked: {
                    var profile = profileRoot.currentProfile()
                    if (profile !== null) {
                        editProfileDialog.profileId = profile.id
                        editProfileDialog.profileName = profile.name
                        editProfileDialog.open()
                    }
                }
            }

            Button {
                text: qsTr("Options")
                enabled: profileGrid.currentItem !== null
                onClicked: {
                    if (profileGrid.currentItem !== null) {
                        profileGrid.currentItem.profileContextMenu.open()
                    }
                }
            }
        }
    }

    NavigableDialog {
        id: editProfileDialog

        property string profileId
        property string profileName

        standardButtons: Dialog.Ok | Dialog.Cancel

        onOpened: {
            nameField.text = profileName
            nameField.forceActiveFocus()
            nameField.selectAll()
        }

        onAccepted: {
            var newName = nameField.text.trim()
            var ok

            if (profileId.length === 0) {
                ok = ProfileManager.createProfile(newName).length > 0
            }
            else {
                ok = ProfileManager.renameProfile(profileId, newName)
            }

            if (!ok) {
                errorDialog.text = qsTr("Profile names must be unique and cannot be empty.")
                errorDialog.open()
            }
        }

        ColumnLayout {
            Label {
                text: editProfileDialog.profileId.length === 0 ? qsTr("New profile name:") : qsTr("Profile name:")
                font.bold: true
            }

            TextField {
                id: nameField
                Layout.fillWidth: true

                Keys.onReturnPressed: editProfileDialog.accept()
                Keys.onEnterPressed: editProfileDialog.accept()
            }
        }
    }

    NavigableMessageDialog {
        id: deleteProfileDialog

        property string profileId
        property string profileName

        standardButtons: Dialog.Yes | Dialog.No
        text: qsTr("Delete profile '%1'? This removes its paired hosts, settings, and identity.").arg(profileName)

        onAccepted: {
            if (!ProfileManager.removeProfile(profileId)) {
                errorDialog.text = qsTr("Unable to delete this profile.")
                errorDialog.open()
            }
        }
    }

    ErrorMessageDialog {
        id: errorDialog
        helpText: ""
    }
}
