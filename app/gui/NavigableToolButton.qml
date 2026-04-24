import QtQuick 2.0
import QtQuick.Controls 2.2
import QtQuick.Controls.Material 2.2
import QtQuick.Layouts 1.3

import StreamingPreferences 1.0

ToolButton {
    property string iconSource

    id: control
    activeFocusOnTab: true

    icon.source: iconSource
    icon.width: background.width
    icon.height: background.height

    // This determines the size of the Material highlight. We increase it
    // from the default because we use larger than normal icons for TV readability.
    Layout.preferredHeight: parent.height

    background: Rectangle {
        color: StreamingPreferences.theme === StreamingPreferences.THEME_OLED ?
                   (control.down ? "#22111111"
                                 : (control.hovered || control.visualFocus ? "#14000000" : "transparent")) :
                   "transparent"
        border.color: control.visualFocus ? Material.accent : "transparent"
        border.width: StreamingPreferences.theme === StreamingPreferences.THEME_OLED ? 1 : 0
        radius: width / 2
    }

    Keys.onReturnPressed: {
        clicked()
    }

    Keys.onEnterPressed: {
        clicked()
    }

    Keys.onRightPressed: {
        nextItemInFocusChain(true).forceActiveFocus(Qt.TabFocus)
    }

    Keys.onLeftPressed: {
        nextItemInFocusChain(false).forceActiveFocus(Qt.TabFocus)
    }
}
