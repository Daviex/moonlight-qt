import QtQuick 2.9
import QtQuick.Controls 2.5
import QtQuick.Window 2.2
import "PopupFocus.js" as PopupFocus

Dialog {
    id: dialog
    modal: true
    parent: Overlay.overlay
    scale: (parent && parent.Window.window && parent.Window.window.uiScaleFactor) || 1
    transformOrigin: Item.TopLeft
    // Qt 6.2 caches the scale used by popup anchors when opening. Explicit
    // centering also follows a live profile scale change while a dialog is open.
    x: parent ? (parent.width - width * scale) / 2 : 0
    y: parent ? (parent.height - height * scale) / 2 : 0
    enter: PopupFade { }
    exit: PopupFade { entering: false }

    // Popups live in Overlay, outside the StackView's key propagation path.
    // Escape is handled by Dialog itself; handle the platform Back key here.
    Shortcut {
        sequence: "Back"
        enabled: dialog.visible && dialog.activeFocus
        autoRepeat: false
        onActivated: PopupFocus.rejectTopModal()
        // Older Qt also registers Back on Popup itself. Both handlers dismiss
        // the same top modal, so accept that native shortcut ambiguity too.
        onActivatedAmbiguously: PopupFocus.rejectTopModal()
    }

    onVisibleChanged: PopupFocus.setDialogVisible(dialog, visible)
    Component.onDestruction: {
        PopupFocus.setDialogVisible(dialog, false)
        if (stackView && stackView.focusViewAfterDialog) {
            PopupFocus.restoreViewFocus(stackView)
        }
    }

    onClosed: {
        // Keep focus in another open popup, such as an error shown while
        // a pairing dialog is closing. Otherwise restore view navigation.
        PopupFocus.restoreViewFocus(stackView)
    }
}
