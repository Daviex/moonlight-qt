import QtQuick 2.9
import QtQuick.Controls 2.2
import QtQuick.Controls.Material 2.2
import QtQuick.Layouts 1.2
import QtQuick.Window 2.2

import StreamingPreferences 1.0
import ComputerManager 1.0
import SdlGamepadKeyNavigation 1.0
import SystemProperties 1.0

Flickable {
    id: settingsPage
    objectName: qsTr("Settings")

    signal languageChanged()

    boundsBehavior: Flickable.OvershootBounds

    contentWidth: settingsColumn1.width > settingsColumn2.width ? settingsColumn1.width : settingsColumn2.width
    contentHeight: settingsColumn1.height > settingsColumn2.height ? settingsColumn1.height : settingsColumn2.height

    ScrollBar.vertical: ScrollBar {
        anchors {
            left: parent.right
            leftMargin: -10
        }
    }

    function isChildOfFlickable(item) {
        while (item) {
            if (item.parent === contentItem) {
                return true
            }

            item = item.parent
        }
        return false
    }

    NumberAnimation on contentY {
        id: autoScrollAnimation
        duration: 100
    }

    Window.onActiveFocusItemChanged: {
        var item = Window.activeFocusItem
        if (item) {
            // Ignore non-child elements like the toolbar buttons
            if (!isChildOfFlickable(item)) {
                return
            }

            // Map the focus item's position into our content item's coordinate space
            var pos = item.mapToItem(contentItem, 0, 0)

            // Ensure some extra space is visible around the element we're scrolling to
            var scrollMargin = height > 100 ? 50 : 0

            if (pos.y - scrollMargin < contentY) {
                autoScrollAnimation.from = contentY
                autoScrollAnimation.to = Math.max(pos.y - scrollMargin, 0)
                autoScrollAnimation.start()
            }
            else if (pos.y + item.height + scrollMargin > contentY + height) {
                autoScrollAnimation.from = contentY
                autoScrollAnimation.to = Math.min(pos.y + item.height + scrollMargin - height, contentHeight - height)
                autoScrollAnimation.start()
            }
        }
    }

    StackView.onActivated: {
        // This enables Tab and BackTab based navigation rather than arrow keys.
        // It is required to shift focus between controls on the settings page.
        SdlGamepadKeyNavigation.setUiNavMode(true)

        // Highlight the first item if a gamepad is connected
        if (SdlGamepadKeyNavigation.getConnectedGamepads() > 0) {
            settingsBasicGroup.resolutionComboBox.forceActiveFocus(Qt.TabFocus)
        }
    }

    StackView.onDeactivating: {
        SdlGamepadKeyNavigation.setUiNavMode(false)

        // Save asynchronously when leaving the page so QSettings flush latency
        // doesn't block navigation. Destruction still performs a synchronous save.
        StreamingPreferences.saveAsync()
    }

    Component.onDestruction: {
        // Also save preferences on destruction, since we won't get a
        // deactivating callback if the user just closes Moonlight
        StreamingPreferences.save()
    }

    Column {
        padding: 10
        id: settingsColumn1
        width: settingsPage.width / 2
        spacing: 15

        SettingsBasicGroup {
            id: settingsBasicGroup
            page: settingsPage
        }

        SettingsUiGroup {
            id: uiSettingsGroupBox
            page: settingsPage
            windowRef: window
        }

        GroupBox {

            id: audioSettingsGroupBox
            width: (parent.width - (parent.leftPadding + parent.rightPadding))
            padding: 12
            title: "<font color=\"" + (StreamingPreferences.theme === StreamingPreferences.THEME_OLED ? Material.accent : "skyblue") + "\">" + qsTr("Audio Settings") + "</font>"
            font.pointSize: 12

            background: Rectangle {
                color: StreamingPreferences.theme === StreamingPreferences.THEME_OLED ? "#050505" : "#303030"
                border.color: "#1A1A1A"
                border.width: StreamingPreferences.theme === StreamingPreferences.THEME_OLED ? 1 : 0
                radius: 8
            }

            Column {
                anchors.fill: parent
                spacing: 5

                Label {
                    width: parent.width
                    id: resAudioTitle
                    text: qsTr("Audio configuration")
                    font.pointSize: 12
                    wrapMode: Text.Wrap
                }

                AutoResizingComboBox {
                    // ignore setting the index at first, and actually set it when the component is loaded
                    Component.onCompleted: {
                        var saved_audio = StreamingPreferences.audioConfig
                        currentIndex = 0
                        for (var i = 0; i < audioListModel.count; i++) {
                            var el_audio = audioListModel.get(i).val;
                            if (saved_audio === el_audio) {
                                currentIndex = i
                                break
                            }
                        }
                        activated(currentIndex)
                    }

                    id: audioComboBox
                    textRole: "text"
                    model: ListModel {
                        id: audioListModel
                        ListElement {
                            text: qsTr("Stereo")
                            val: StreamingPreferences.AC_STEREO
                        }
                        ListElement {
                            text: qsTr("5.1 surround sound")
                            val: StreamingPreferences.AC_51_SURROUND
                        }
                        ListElement {
                            text: qsTr("7.1 surround sound")
                            val: StreamingPreferences.AC_71_SURROUND
                        }
                    }
                    // ::onActivated must be used, as it only listens for when the index is changed by a human
                    onActivated : {
                        StreamingPreferences.audioConfig = audioListModel.get(currentIndex).val
                    }
                }


                CheckBox {
                    id: audioPcCheck
                    width: parent.width
                    text: qsTr("Mute host PC speakers while streaming")
                    font.pointSize: 12
                    checked: !StreamingPreferences.playAudioOnHost
                    onCheckedChanged: {
                        StreamingPreferences.playAudioOnHost = !checked
                    }

                    ToolTip.delay: 1000
                    ToolTip.timeout: 5000
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("You must restart any game currently in progress for this setting to take effect")
                }

                CheckBox {
                    id: muteOnFocusLossCheck
                    width: parent.width
                    text: qsTr("Mute audio stream when Moonlight is not the active window")
                    font.pointSize: 12
                    visible: SystemProperties.hasDesktopEnvironment
                    checked: StreamingPreferences.muteOnFocusLoss
                    onCheckedChanged: {
                        StreamingPreferences.muteOnFocusLoss = checked
                    }

                    ToolTip.delay: 1000
                    ToolTip.timeout: 5000
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Mutes Moonlight's audio when you Alt+Tab out of the stream or click on a different window.")
                }
            }
        }

        GroupBox {
            id: hostSettingsGroupBox
            width: (parent.width - (parent.leftPadding + parent.rightPadding))
            padding: 12
            title: "<font color=\"" + (StreamingPreferences.theme === StreamingPreferences.THEME_OLED ? Material.accent : "skyblue") + "\">" + qsTr("Host Settings") + "</font>"
            font.pointSize: 12

            background: Rectangle {
                color: StreamingPreferences.theme === StreamingPreferences.THEME_OLED ? "#050505" : "#303030"
                border.color: "#1A1A1A"
                border.width: StreamingPreferences.theme === StreamingPreferences.THEME_OLED ? 1 : 0
                radius: 8
            }

            Column {
                anchors.fill: parent
                spacing: 5

                CheckBox {
                    id: optimizeGameSettingsCheck
                    width: parent.width
                    text: qsTr("Optimize game settings for streaming")
                    font.pointSize:  12
                    checked: StreamingPreferences.gameOptimizations
                    onCheckedChanged: {
                        StreamingPreferences.gameOptimizations = checked
                    }
                }

                CheckBox {
                    id: quitAppAfter
                    width: parent.width
                    text: qsTr("Quit app on host PC after ending stream")
                    font.pointSize: 12
                    checked: StreamingPreferences.quitAppAfter
                    onCheckedChanged: {
                        StreamingPreferences.quitAppAfter = checked
                    }

                    ToolTip.delay: 1000
                    ToolTip.timeout: 5000
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("This will close the app or game you are streaming when you end your stream. You will lose any unsaved progress!")
                }
            }
        }

    }

    Column {
        padding: 10
        rightPadding: 20
        anchors.left: settingsColumn1.right
        id: settingsColumn2
        width: settingsPage.width / 2
        spacing: 15

        GroupBox {
            id: inputSettingsGroupBox
            width: (parent.width - (parent.leftPadding + parent.rightPadding))
            padding: 12
            title: "<font color=\"" + (StreamingPreferences.theme === StreamingPreferences.THEME_OLED ? Material.accent : "skyblue") + "\">" + qsTr("Input Settings") + "</font>"
            font.pointSize: 12

            background: Rectangle {
                color: StreamingPreferences.theme === StreamingPreferences.THEME_OLED ? "#050505" : "#303030"
                border.color: "#1A1A1A"
                border.width: StreamingPreferences.theme === StreamingPreferences.THEME_OLED ? 1 : 0
                radius: 8
            }

            Column {
                anchors.fill: parent
                spacing: 5

                CheckBox {
                    id: absoluteMouseCheck
                    hoverEnabled: true
                    width: parent.width
                    text: qsTr("Optimize mouse for remote desktop instead of games")
                    font.pointSize:  12
                    checked: StreamingPreferences.absoluteMouseMode
                    onCheckedChanged: {
                        StreamingPreferences.absoluteMouseMode = checked
                    }

                    ToolTip.delay: 1000
                    ToolTip.timeout: 10000
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("This enables seamless mouse control without capturing the client's mouse cursor. It is ideal for remote desktop usage but will not work in most games.") + " " +
                                  qsTr("You can toggle this while streaming using Ctrl+Alt+Shift+M.") + "\n\n" +
                                  qsTr("NOTE: Due to a bug in GeForce Experience, this option may not work properly if your host PC has multiple monitors.")
                }

                Row {
                    spacing: 5
                    width: parent.width

                    CheckBox {
                        id: captureSysKeysCheck
                        hoverEnabled: true
                        text: qsTr("Capture system keyboard shortcuts")
                        font.pointSize: 12
                        enabled: SystemProperties.hasDesktopEnvironment
                        checked: StreamingPreferences.captureSysKeysMode !== StreamingPreferences.CSK_OFF || !SystemProperties.hasDesktopEnvironment

                        ToolTip.delay: 1000
                        ToolTip.timeout: 10000
                        ToolTip.visible: hovered
                        ToolTip.text: qsTr("This enables the capture of system-wide keyboard shortcuts like Alt+Tab that would normally be handled by the client OS while streaming.") + "\n\n" +
                                      qsTr("NOTE: Certain keyboard shortcuts like Ctrl+Alt+Del on Windows cannot be intercepted by any application, including Moonlight.")
                    }

                    AutoResizingComboBox {
                        // ignore setting the index at first, and actually set it when the component is loaded
                        Component.onCompleted: {
                            if (!visible) {
                                // Do nothing if the control won't even be visible
                                return
                            }

                            var saved_syskeysmode = StreamingPreferences.captureSysKeysMode
                            currentIndex = 0
                            for (var i = 0; i < captureSysKeysModeListModel.count; i++) {
                                var el_syskeysmode = captureSysKeysModeListModel.get(i).val;
                                if (saved_syskeysmode === el_syskeysmode) {
                                    currentIndex = i
                                    break
                                }
                            }

                            activated(currentIndex)
                        }

                        enabled: captureSysKeysCheck.checked && captureSysKeysCheck.enabled
                        textRole: "text"
                        model: ListModel {
                            id: captureSysKeysModeListModel
                            ListElement {
                                text: qsTr("in fullscreen")
                                val: StreamingPreferences.CSK_FULLSCREEN
                            }
                            ListElement {
                                text: qsTr("always")
                                val: StreamingPreferences.CSK_ALWAYS
                            }
                        }

                        function updatePref() {
                            if (!enabled) {
                                StreamingPreferences.captureSysKeysMode = StreamingPreferences.CSK_OFF
                            }
                            else {
                                StreamingPreferences.captureSysKeysMode = captureSysKeysModeListModel.get(currentIndex).val
                            }
                        }

                        // ::onActivated must be used, as it only listens for when the index is changed by a human
                        onActivated: {
                            updatePref()
                        }

                        // This handles transition of the checkbox state
                        onEnabledChanged: {
                            updatePref()
                        }
                    }
                }

                CheckBox {
                    id: absoluteTouchCheck
                    hoverEnabled: true
                    width: parent.width
                    text: qsTr("Use touchscreen as a virtual trackpad")
                    font.pointSize:  12
                    checked: !StreamingPreferences.absoluteTouchMode
                    onCheckedChanged: {
                        StreamingPreferences.absoluteTouchMode = !checked
                    }

                    ToolTip.delay: 1000
                    ToolTip.timeout: 5000
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("When checked, the touchscreen acts like a trackpad. When unchecked, the touchscreen will directly control the mouse pointer.")
                }

                CheckBox {
                    id: swapMouseButtonsCheck
                    hoverEnabled: true
                    width: parent.width
                    text: qsTr("Swap left and right mouse buttons")
                    font.pointSize:  12
                    checked: StreamingPreferences.swapMouseButtons
                    onCheckedChanged: {
                        StreamingPreferences.swapMouseButtons = checked
                    }
                }

                CheckBox {
                    id: reverseScrollButtonsCheck
                    hoverEnabled: true
                    width: parent.width
                    text: qsTr("Reverse mouse scrolling direction")
                    font.pointSize: 12
                    checked: StreamingPreferences.reverseScrollDirection
                    onCheckedChanged: {
                        StreamingPreferences.reverseScrollDirection = checked
                    }
                }
            }
        }

        GroupBox {
            id: gamepadSettingsGroupBox
            width: (parent.width - (parent.leftPadding + parent.rightPadding))
            padding: 12
            title: "<font color=\"" + (StreamingPreferences.theme === StreamingPreferences.THEME_OLED ? Material.accent : "skyblue") + "\">" + qsTr("Gamepad Settings") + "</font>"
            font.pointSize: 12

            background: Rectangle {
                color: StreamingPreferences.theme === StreamingPreferences.THEME_OLED ? "#050505" : "#303030"
                border.color: "#1A1A1A"
                border.width: StreamingPreferences.theme === StreamingPreferences.THEME_OLED ? 1 : 0
                radius: 8
            }

            Column {
                anchors.fill: parent
                spacing: 5

                CheckBox {
                    id: swapFaceButtonsCheck
                    width: parent.width
                    text: qsTr("Swap A/B and X/Y gamepad buttons")
                    font.pointSize: 12
                    checked: StreamingPreferences.swapFaceButtons
                    onCheckedChanged: {
                        StreamingPreferences.swapFaceButtons = checked
                    }

                    ToolTip.delay: 1000
                    ToolTip.timeout: 5000
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("This switches gamepads into a Nintendo-style button layout")
                }

                CheckBox {
                    id: singleControllerCheck
                    width: parent.width
                    text: qsTr("Force gamepad #1 always connected")
                    font.pointSize:  12
                    checked: !StreamingPreferences.multiController
                    onCheckedChanged: {
                        StreamingPreferences.multiController = !checked
                    }

                    ToolTip.delay: 1000
                    ToolTip.timeout: 5000
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Forces a single gamepad to always stay connected to the host, even if no gamepads are actually connected to this PC.") + " " +
                                  qsTr("Only enable this option when streaming a game that doesn't support gamepads being connected after startup.")
                }

                CheckBox {
                    id: gamepadMouseCheck
                    hoverEnabled: true
                    width: parent.width
                    text: qsTr("Enable mouse control with gamepads by holding the 'Start' button")
                    font.pointSize: 12
                    checked: StreamingPreferences.gamepadMouse
                    onCheckedChanged: {
                        StreamingPreferences.gamepadMouse = checked
                    }
                }

                CheckBox {
                    id: backgroundGamepadCheck
                    width: parent.width
                    text: qsTr("Process gamepad input when Moonlight is in the background")
                    font.pointSize: 12
                    visible: SystemProperties.hasDesktopEnvironment
                    checked: StreamingPreferences.backgroundGamepad
                    onCheckedChanged: {
                        StreamingPreferences.backgroundGamepad = checked
                    }

                    ToolTip.delay: 1000
                    ToolTip.timeout: 5000
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("Allows Moonlight to capture gamepad inputs even if it's not the current window in focus")
                }
            }
        }

        SettingsAdvancedGroup {
            id: advancedSettingsGroupBox
            windowRef: window
            bitrateSlider: settingsBasicGroup.bitrateSlider
        }
    }
}
