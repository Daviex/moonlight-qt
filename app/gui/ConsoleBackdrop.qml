import QtQuick 2.9
import QtQuick.Window 2.2
import StreamingPreferences 1.0
import SystemProperties 1.0
import "ConsoleStyle.js" as Style

Item {
    id: backdrop
    property url artwork: ""
    property url settledArtwork: ""
    readonly property bool effectsAvailable: SystemProperties.supportsQuickEffects &&
                                             GraphicsInfo.api !== GraphicsInfo.Software
    clip: true
    Rectangle { anchors.fill: parent; color: Style.background }
    onArtworkChanged: settle.restart()
    Timer { id: settle; interval: StreamingPreferences.uiReducedMotion ? 0 : 150; onTriggered: backdrop.settledArtwork = backdrop.artwork }
    Loader {
        anchors.fill: parent
        active: backdrop.effectsAvailable && !Style.placeholder(backdrop.settledArtwork)
        // QtQuick.Effects starts at Qt 6.5. Keep the optional import in a
        // separately loaded file so Qt 6.2 can still instantiate the page.
        source: "ConsoleBackdropEffect.qml"
        onLoaded: item.artwork = Qt.binding(function() { return backdrop.settledArtwork })
    }
}
