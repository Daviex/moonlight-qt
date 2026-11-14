import QtQuick 2.9

// Keep the UI scale stable while opening/closing. Material's default scale
// animations end at 1.0, which would override the selected profile's scale.
Transition {
    id: transition
    property bool entering: true

    NumberAnimation {
        property: "opacity"
        from: transition.entering ? 0 : 1
        to: transition.entering ? 1 : 0
        duration: 150
        easing.type: Easing.OutCubic
    }
}
