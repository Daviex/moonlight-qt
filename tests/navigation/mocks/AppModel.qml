import QtQuick 2.9

ListModel {
    signal computerLost()
    signal hostSleepCompleted(string error)
    function initialize(manager, computerIndex, showHidden) {
        populate(manager.appCount)
    }
    function populate(appCount) {
        clear()
        for (var i = 0; i < appCount; i++) {
            append({ appid: i + 1, name: i === 0 ? "Test Game" : "Test Game " + (i + 1), running: false, hidden: false,
                     sleepAction: false,
                     directLaunch: false, isAppCollectorGame: false, customStreamingSettings: false,
                     boxart: "qrc:/res/no_app_image.png" })
        }
    }
    function getDirectLaunchAppIndex() { return -1 }
    function indexOfApp(appId) {
        for (var i = 0; i < count; i++) if (get(i).appid === appId) return i
        return -1
    }
    function createGameSettings(appId) { return gameSettingsFactory.create(appId, this) }
    function removeGameSettings(appId) {
        var result = gameSettingsFactory.remove(appId)
        setProperty(indexOfApp(appId), "customStreamingSettings", false)
        return result
    }
}
