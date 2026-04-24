import QtQuick 2.0
import QtQuick.Controls 2.2
import QtQuick.Layouts 1.2

NavigableDialog {
    id: renameDialog
    property var computerModel
    property string label: qsTr("Enter the new name for this PC:")
    property string originalName
    property int pcIndex: -1

    standardButtons: Dialog.Ok | Dialog.Cancel

    onOpened: {
        editText.forceActiveFocus()
    }

    onClosed: {
        editText.clear()
    }

    onAccepted: {
        if (editText.text) {
            computerModel.renameComputer(pcIndex, editText.text)
        }
    }

    ColumnLayout {
        Label {
            text: renameDialog.label
            font.bold: true
        }

        TextField {
            id: editText
            placeholderText: renameDialog.originalName
            Layout.fillWidth: true
            focus: true

            Keys.onReturnPressed: {
                renameDialog.accept()
            }

            Keys.onEnterPressed: {
                renameDialog.accept()
            }
        }
    }
}
