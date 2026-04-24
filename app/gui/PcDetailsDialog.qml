import QtQuick 2.0
import QtQuick.Controls 2.2

NavigableMessageDialog {
    property string pcDetails: ""

    text: pcDetails
    imageSrc: "qrc:/res/baseline-help_outline-24px.svg"
    standardButtons: Dialog.Ok
}
