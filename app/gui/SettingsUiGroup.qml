import QtQuick 2.9
import QtQuick.Controls 2.2
import QtQuick.Controls.Material 2.2

import StreamingPreferences 1.0
import SystemProperties 1.0

GroupBox {
    property var page
    property var windowRef

    width: parent.width - (parent.leftPadding + parent.rightPadding)
    padding: 12
    title: "<font color=\"" + (StreamingPreferences.theme === StreamingPreferences.THEME_OLED ? Material.accent : "skyblue") + "\">" + qsTr("UI Settings") + "</font>"
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
            id: themeTitle
            text: qsTr("Theme")
            font.pointSize: 12
            wrapMode: Text.Wrap
        }

        AutoResizingComboBox {
            Component.onCompleted: {
                var saved_theme = StreamingPreferences.theme
                currentIndex = 0
                for (var i = 0; i < themeListModel.count; i++) {
                    var el_theme = themeListModel.get(i).val
                    if (saved_theme === el_theme) {
                        currentIndex = i
                        break
                    }
                }

                activated(currentIndex)
            }

            id: themeComboBox
            textRole: "text"
            model: ListModel {
                id: themeListModel
                ListElement {
                    text: qsTr("Default")
                    val: StreamingPreferences.THEME_DEFAULT
                }
                ListElement {
                    text: qsTr("OLED")
                    val: StreamingPreferences.THEME_OLED
                }
            }

            onActivated: {
                var new_theme = themeListModel.get(currentIndex).val
                if (StreamingPreferences.theme !== new_theme) {
                    StreamingPreferences.theme = new_theme
                }
            }
        }

        Label {
            width: parent.width
            id: languageTitle
            text: qsTr("Language")
            font.pointSize: 12
            wrapMode: Text.Wrap
        }

        AutoResizingComboBox {
            Component.onCompleted: {
                var saved_language = StreamingPreferences.language
                currentIndex = 0
                for (var i = 0; i < languageListModel.count; i++) {
                    var el_language = languageListModel.get(i).val
                    if (saved_language === el_language) {
                        currentIndex = i
                        break
                    }
                }

                activated(currentIndex)
            }

            id: languageComboBox
            textRole: "text"
            model: ListModel {
                id: languageListModel
                ListElement {
                    text: qsTr("Automatic")
                    val: StreamingPreferences.LANG_AUTO
                }
                ListElement {
                    text: "Deutsch"
                    val: StreamingPreferences.LANG_DE
                }
                ListElement {
                    text: "English"
                    val: StreamingPreferences.LANG_EN
                }
                ListElement {
                    text: "Français"
                    val: StreamingPreferences.LANG_FR
                }
                ListElement {
                    text: "简体中文"
                    val: StreamingPreferences.LANG_ZH_CN
                }
                ListElement {
                    text: "Norwegian Bokmål"
                    val: StreamingPreferences.LANG_NB_NO
                }
                ListElement {
                    text: "русский"
                    val: StreamingPreferences.LANG_RU
                }
                ListElement {
                    text: "Español"
                    val: StreamingPreferences.LANG_ES
                }
                ListElement {
                    text: "日本語"
                    val: StreamingPreferences.LANG_JA
                }
                ListElement {
                    text: "Tiếng Việt"
                    val: StreamingPreferences.LANG_VI
                }
                ListElement {
                    text: "ภาษาไทย"
                    val: StreamingPreferences.LANG_TH
                }
                ListElement {
                    text: "한국어"
                    val: StreamingPreferences.LANG_KO
                }
                ListElement {
                    text: "Magyar"
                    val: StreamingPreferences.LANG_HU
                }
                ListElement {
                    text: "Nederlands"
                    val: StreamingPreferences.LANG_NL
                }
                ListElement {
                    text: "Svenska"
                    val: StreamingPreferences.LANG_SV
                }
                ListElement {
                    text: "Türkçe"
                    val: StreamingPreferences.LANG_TR
                }
                /* ListElement {
                    text: "Українська"
                    val: StreamingPreferences.LANG_UK
                } */
                ListElement {
                    text: "繁體中文"
                    val: StreamingPreferences.LANG_ZH_TW
                }
                ListElement {
                    text: "Português"
                    val: StreamingPreferences.LANG_PT
                }
                ListElement {
                    text: "Português do Brasil"
                    val: StreamingPreferences.LANG_PT_BR
                }
                ListElement {
                    text: "Ελληνικά"
                    val: StreamingPreferences.LANG_EL
                }
                ListElement {
                    text: "Italiano"
                    val: StreamingPreferences.LANG_IT
                }
                /* ListElement {
                    text: "हिन्दी, हिंदी"
                    val: StreamingPreferences.LANG_HI
                } */
                ListElement {
                    text: "Język polski"
                    val: StreamingPreferences.LANG_PL
                }
                ListElement {
                    text: "Čeština"
                    val: StreamingPreferences.LANG_CS
                }
                /* ListElement {
                    text: "עִבְרִית"
                    val: StreamingPreferences.LANG_HE
                } */
                /* ListElement {
                    text: "کرمانجیی خواروو"
                    val: StreamingPreferences.LANG_CKB
                } */
                /* ListElement {
                    text: "Lietuvių kalba"
                    val: StreamingPreferences.LANG_LT
                } */
                /* ListElement {
                    text: "Eesti"
                    val: StreamingPreferences.LANG_ET
                } */
                ListElement {
                    text: "Български"
                    val: StreamingPreferences.LANG_BG
                }
                /* ListElement {
                    text: "Esperanto"
                    val: StreamingPreferences.LANG_EO
                } */
                ListElement {
                    text: "தமிழ்"
                    val: StreamingPreferences.LANG_TA
                }
            }

            onActivated: {
                var new_language = languageListModel.get(currentIndex).val
                if (StreamingPreferences.language !== new_language) {
                    StreamingPreferences.language = languageListModel.get(currentIndex).val
                    if (!StreamingPreferences.retranslate()) {
                        ToolTip.show(qsTr("You must restart Moonlight for this change to take effect"), 5000)
                    }
                    else {
                        windowRef.prepareForRetranslateBackNavigation()
                        page.languageChanged()
                    }
                }
            }
        }

        Label {
            width: parent.width
            id: uiDisplayModeTitle
            text: qsTr("GUI display mode")
            font.pointSize: 12
            wrapMode: Text.Wrap
            visible: SystemProperties.hasDesktopEnvironment
        }

        AutoResizingComboBox {
            Component.onCompleted: {
                if (!visible) {
                    return
                }

                var saved_uidisplaymode = StreamingPreferences.uiDisplayMode
                currentIndex = 0
                for (var i = 0; i < uiDisplayModeListModel.count; i++) {
                    var el_uidisplaymode = uiDisplayModeListModel.get(i).val
                    if (saved_uidisplaymode === el_uidisplaymode) {
                        currentIndex = i
                        break
                    }
                }

                activated(currentIndex)
            }

            id: uiDisplayModeComboBox
            visible: SystemProperties.hasDesktopEnvironment
            textRole: "text"
            model: ListModel {
                id: uiDisplayModeListModel
                ListElement {
                    text: qsTr("Windowed")
                    val: StreamingPreferences.UI_WINDOWED
                }
                ListElement {
                    text: qsTr("Maximized")
                    val: StreamingPreferences.UI_MAXIMIZED
                }
                ListElement {
                    text: qsTr("Fullscreen")
                    val: StreamingPreferences.UI_FULLSCREEN
                }
            }

            onActivated: {
                StreamingPreferences.uiDisplayMode = uiDisplayModeListModel.get(currentIndex).val
            }
        }

        CheckBox {
            id: connectionWarningsCheck
            width: parent.width
            text: qsTr("Show connection quality warnings")
            font.pointSize: 12
            checked: StreamingPreferences.connectionWarnings

            onCheckedChanged: {
                StreamingPreferences.connectionWarnings = checked
            }
        }

        CheckBox {
            id: configurationWarningsCheck
            width: parent.width
            text: qsTr("Show configuration warnings")
            font.pointSize: 12
            checked: StreamingPreferences.configurationWarnings

            onCheckedChanged: {
                StreamingPreferences.configurationWarnings = checked
            }
        }

        CheckBox {
            visible: SystemProperties.hasDiscordIntegration
            id: discordPresenceCheck
            width: parent.width
            text: qsTr("Discord Rich Presence integration")
            font.pointSize: 12
            checked: StreamingPreferences.richPresence

            onCheckedChanged: {
                StreamingPreferences.richPresence = checked
            }

            ToolTip.delay: 1000
            ToolTip.timeout: 5000
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Updates your Discord status to display the name of the game you're streaming.")
        }

        CheckBox {
            id: keepAwakeCheck
            width: parent.width
            text: qsTr("Keep the display awake while streaming")
            font.pointSize: 12
            checked: StreamingPreferences.keepAwake

            onCheckedChanged: {
                StreamingPreferences.keepAwake = checked
            }

            ToolTip.delay: 1000
            ToolTip.timeout: 5000
            ToolTip.visible: hovered
            ToolTip.text: qsTr("Prevents the screensaver from starting or the display from going to sleep while streaming.")
        }
    }
}
