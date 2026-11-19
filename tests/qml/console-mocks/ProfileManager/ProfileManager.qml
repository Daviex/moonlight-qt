pragma Singleton
import QtQuick 2.9

QtObject {
    property bool hasActiveProfile: true
    property string activeProfileId: "default"
    property string activeProfileName: "Test profile"
    property string defaultProfileId: "default"
    property string autoLoginProfileId: ""
    property bool autoLoginEnabled: false
    property var profiles: [{ id: "default", name: "Test profile", iconId: "default" }]
    signal activeProfileAboutToChange()
    signal activeProfileChanged()
    function activateProfile(id) { activeProfileId = id; hasActiveProfile = true; return true }
    function setAutoLoginProfile(id, enabled) { autoLoginProfileId = id; autoLoginEnabled = enabled }
    function setAutoLoginEnabled(enabled) { autoLoginEnabled = enabled }
    function setDefaultProfile(id) { defaultProfileId = id }
}
