use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeEvent {
    pub kind: BridgeEventKind,
    pub message: String,
    pub host_id: Option<String>,
    pub app_id: Option<String>,
    pub controller_action: Option<ControllerAction>,
    pub update_version: Option<String>,
    pub update_url: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BridgeEventKind {
    HostChanged,
    AppChanged,
    SessionChanged,
    SettingsChanged,
    Status,
    ControllerAction,
    UpdateAvailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ControllerAction {
    Up,
    Down,
    Left,
    Right,
    Accept,
    Back,
    ContextMenu,
    Settings,
    NextControl,
    PreviousControl,
    ActivateControl,
}

#[cfg(test)]
mod tests {
    use super::{BridgeEvent, BridgeEventKind, ControllerAction};

    #[test]
    fn event_kind_uses_camel_case_json() {
        let json = serde_json::to_string(&BridgeEventKind::SessionChanged).unwrap();

        assert_eq!(r#""sessionChanged""#, json);
    }

    #[test]
    fn controller_action_uses_camel_case_json() {
        let json = serde_json::to_string(&ControllerAction::NextControl).unwrap();

        assert_eq!(r#""nextControl""#, json);
    }

    #[test]
    fn bridge_event_serializes_existing_field_names() {
        let event = BridgeEvent {
            kind: BridgeEventKind::HostChanged,
            message: "Host changed.".into(),
            host_id: Some("gaming-pc".into()),
            app_id: None,
            controller_action: None,
            update_version: None,
            update_url: None,
        };

        let value = serde_json::to_value(event).unwrap();

        assert_eq!(value["kind"], "hostChanged");
        assert_eq!(value["message"], "Host changed.");
        assert_eq!(value["hostId"], "gaming-pc");
    }
}
