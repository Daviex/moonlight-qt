pragma Singleton
import QtQuick 2.9
QtObject {
    signal computerAddCompleted(bool success, bool portBlocking)
    signal quitAppCompleted(var error)
    function addNewHostManually(address) {}
    function reloadForActiveProfile() {}
 function startPolling() {} function stopPollingAsync() {} }
