import QtQuick 2.0
import QtQuick.Controls 2.2

NavigableMessageDialog {
    property var computerModel
    property int pcIndex: -1
    property string pcName: ""

    text: qsTr("Are you sure you want to remove '%1'?").arg(pcName)
    standardButtons: Dialog.Yes | Dialog.No

    onAccepted: {
        computerModel.deleteComputer(pcIndex)
    }
}
