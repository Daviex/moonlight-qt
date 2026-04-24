import QtQuick 2.0
import QtQuick.Controls 2.5
import QtQuick.Controls.Material 2.2

import StreamingPreferences 1.0

Dialog {
    modal: true
    dim: true
    anchors.centerIn: Overlay.overlay
    padding: 20

    Overlay.modal: Rectangle {
        color: StreamingPreferences.theme === StreamingPreferences.THEME_OLED ? "#E6000000" : "#CC000000"
    }

    background: Rectangle {
        color: StreamingPreferences.theme === StreamingPreferences.THEME_OLED ? "#000000" : "#303030"
        border.color: Material.accent
        border.width: StreamingPreferences.theme === StreamingPreferences.THEME_OLED ? 1 : 0
        radius: 10
    }

    onClosed: {
        // We must force focus back to the last item. If we don't,
        // gamepad and keyboard navigation will break after a
        // dialog appears.
        stackView.forceActiveFocus()
    }
}
