import QtQuick 2.9
import QtQuick.Controls 2.2
import QtQuick.Window 2.2

import SdlGamepadKeyNavigation 1.0

// https://stackoverflow.com/questions/45029968/how-do-i-set-the-combobox-width-to-fit-the-largest-item
ComboBox {
    id: control
    property int textWidth
    property int desiredWidth : leftPadding + textWidth + indicator.width + rightPadding
    property int maximumWidth : parent.width

    implicitWidth: desiredWidth < maximumWidth ? desiredWidth : maximumWidth

    popup.scale: (control.Window.window && control.Window.window.uiScaleFactor) || 1
    popup.transformOrigin: Item.TopLeft
    popup.enter: PopupFade { }
    popup.exit: PopupFade { entering: false }
    // The Material popup's explicit height uses window units. Convert its
    // available space to UI units so long lists remain scrollable at 200%.
    popup.height: Math.min(popup.contentItem.implicitHeight + popup.topPadding + popup.bottomPadding,
                           Math.max(0, control.Window.height - Math.max(0, popup.topMargin) -
                                    Math.max(0, popup.bottomMargin)) / popup.scale)

    // Keep the menu opaque, including software-rendered Qt Quick previews.
    // The Material elevation layer can otherwise hide the menu background.
    popup.background: Rectangle {
        color: "#383838"
        radius: 6
        border.color: "#707070"
    }

    TextMetrics {
        id: popupMetrics
    }

    TextMetrics {
        id: textMetrics
    }

    function recalculateWidth() {
        textMetrics.font = font
        popupMetrics.font = popup.font
        textWidth = 0
        for (var i = 0; i < count; i++){
            textMetrics.text = textAt(i)
            popupMetrics.text = textAt(i)
            textWidth = Math.max(textMetrics.width, textWidth)
            textWidth = Math.max(popupMetrics.width, textWidth)
        }
    }

    // We call this every time the options change (and init)
    // so we can adjust the combo box width here too
    onActivated: recalculateWidth()

    popup.onAboutToShow: {
        // Switch to normal navigation for combo boxes
        SdlGamepadKeyNavigation.setUiNavMode(false)
    }

    popup.onAboutToHide: {
        SdlGamepadKeyNavigation.setUiNavMode(true)
    }

    Keys.onLeftPressed: {
        if (!popup.visible) decrementCurrentIndex()
    }

    Keys.onRightPressed: {
        if (!popup.visible) incrementCurrentIndex()
    }
}
