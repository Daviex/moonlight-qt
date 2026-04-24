import QtQuick 2.0
import QtQuick.Controls 2.2

NavigableMessageDialog {
    id: dialog
    closePolicy: Popup.CloseOnEscape
    standardButtons: Dialog.Ok

    onAboutToShow: {
        dialog.text = qsTr("Moonlight is testing your network connection to determine if any required ports are blocked.") + "\n\n" + qsTr("This may take a few seconds…")
        showSpinner = true
    }

    function connectionTestComplete(result, blockedPorts)
    {
        if (result === -1) {
            text = qsTr("The network test could not be performed because none of Moonlight's connection testing servers were reachable from this PC. Check your Internet connection or try again later.")
            imageSrc = "qrc:/res/baseline-warning-24px.svg"
        }
        else if (result === 0) {
            text = qsTr("This network does not appear to be blocking Moonlight. If you still have trouble connecting, check your PC's firewall settings.") + "\n\n" + qsTr("If you are trying to stream over the Internet, install the Moonlight Internet Hosting Tool on your gaming PC and run the included Internet Streaming Tester to check your gaming PC's Internet connection.")
            imageSrc = "qrc:/res/baseline-check_circle_outline-24px.svg"
        }
        else {
            text = qsTr("Your PC's current network connection seems to be blocking Moonlight. Streaming over the Internet may not work while connected to this network.") + "\n\n" + qsTr("The following network ports were blocked:") + "\n"
            text += blockedPorts
            imageSrc = "qrc:/res/baseline-error_outline-24px.svg"
        }

        showSpinner = false
    }
}
