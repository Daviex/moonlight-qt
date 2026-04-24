import QtQuick 2.0
import QtQuick.Controls 2.2

NavigableMessageDialog {
    closePolicy: Popup.CloseOnEscape

    property string pin: "0000"

    text: qsTr("Please enter %1 on your host PC. This dialog will close when pairing is completed.").arg(pin) + "\n\n" +
          qsTr("If your host PC is running Sunshine, navigate to the Sunshine web UI to enter the PIN.")
    standardButtons: Dialog.Cancel

    onRejected: {
        // FIXME: We should interrupt pairing here
    }
}
