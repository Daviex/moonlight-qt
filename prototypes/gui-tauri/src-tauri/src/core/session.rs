#![allow(dead_code)]

use super::error::CoreError;
use super::events::{BridgeEvent, BridgeEventKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionSummary {
    pub active: bool,
    pub host_id: Option<String>,
    pub app_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SessionState {
    Idle,
    Launching { host_id: String, app_id: String },
    Active { host_id: String, app_id: String },
    HideUiRequested { host_id: String, app_id: String },
    Quitting { host_id: String, app_id: String },
    Finished,
    Failed { message: String },
}

impl SessionState {
    pub fn status(&self) -> SessionSummary {
        match self {
            Self::Idle | Self::Finished | Self::Failed { .. } => SessionSummary {
                active: false,
                host_id: None,
                app_id: None,
            },
            Self::Launching { host_id, app_id }
            | Self::Active { host_id, app_id }
            | Self::HideUiRequested { host_id, app_id }
            | Self::Quitting { host_id, app_id } => SessionSummary {
                active: matches!(self, Self::Active { .. } | Self::HideUiRequested { .. }),
                host_id: Some(host_id.clone()),
                app_id: Some(app_id.clone()),
            },
        }
    }

    pub fn event(&self) -> BridgeEvent {
        let status = self.status();
        BridgeEvent {
            kind: BridgeEventKind::SessionChanged,
            message: if status.active {
                "Stream session active.".into()
            } else {
                "Stream session inactive.".into()
            },
            host_id: status.host_id,
            app_id: status.app_id,
            controller_action: None,
            update_version: None,
            update_url: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SessionMachine {
    state: SessionState,
}

impl Default for SessionMachine {
    fn default() -> Self {
        Self {
            state: SessionState::Idle,
        }
    }
}

impl SessionMachine {
    pub fn state(&self) -> &SessionState {
        &self.state
    }

    pub fn launch(
        &mut self,
        host_id: impl Into<String>,
        app_id: impl Into<String>,
    ) -> Result<(), CoreError> {
        if !matches!(
            self.state,
            SessionState::Idle | SessionState::Finished | SessionState::Failed { .. }
        ) {
            return Err(CoreError::Validation(
                "A stream session is already running.".into(),
            ));
        }

        let host_id = host_id.into();
        let app_id = app_id.into();
        if host_id.trim().is_empty() {
            return Err(CoreError::Validation(
                "Host ID is required to start a stream.".into(),
            ));
        }
        if app_id.trim().is_empty() {
            return Err(CoreError::Validation(
                "App ID is required to start a stream.".into(),
            ));
        }

        self.state = SessionState::Launching { host_id, app_id };
        Ok(())
    }

    pub fn mark_active(&mut self) -> Result<(), CoreError> {
        let SessionState::Launching { host_id, app_id } = self.state.clone() else {
            return Err(CoreError::Validation(
                "Stream can only become active after launching.".into(),
            ));
        };

        self.state = SessionState::Active { host_id, app_id };
        Ok(())
    }

    pub fn request_hide_ui(&mut self) -> Result<(), CoreError> {
        let SessionState::Active { host_id, app_id } = self.state.clone() else {
            return Err(CoreError::Validation(
                "UI can only be hidden for an active stream.".into(),
            ));
        };

        self.state = SessionState::HideUiRequested { host_id, app_id };
        Ok(())
    }

    pub fn quit(&mut self) -> Result<(), CoreError> {
        match self.state.clone() {
            SessionState::Launching { host_id, app_id }
            | SessionState::Active { host_id, app_id }
            | SessionState::HideUiRequested { host_id, app_id } => {
                self.state = SessionState::Quitting { host_id, app_id };
                Ok(())
            }
            _ => Err(CoreError::Validation(
                "No active stream session to quit.".into(),
            )),
        }
    }

    pub fn finish(&mut self) {
        self.state = SessionState::Finished;
    }

    pub fn fail(&mut self, message: impl Into<String>) {
        self.state = SessionState::Failed {
            message: message.into(),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::{SessionMachine, SessionState};
    use crate::core::events::BridgeEventKind;

    #[test]
    fn session_launch_requires_ids() {
        let mut session = SessionMachine::default();
        let error = session.launch("", "123").unwrap_err();

        assert_eq!("Host ID is required to start a stream.", error.to_string());
    }

    #[test]
    fn session_moves_through_launch_active_hide_and_quit() {
        let mut session = SessionMachine::default();

        session.launch("host-1", "app-1").unwrap();
        assert!(matches!(session.state(), SessionState::Launching { .. }));

        session.mark_active().unwrap();
        assert!(matches!(session.state(), SessionState::Active { .. }));
        assert!(session.state().status().active);

        session.request_hide_ui().unwrap();
        assert!(matches!(
            session.state(),
            SessionState::HideUiRequested { .. }
        ));

        session.quit().unwrap();
        assert!(matches!(session.state(), SessionState::Quitting { .. }));
    }

    #[test]
    fn session_rejects_double_launch() {
        let mut session = SessionMachine::default();
        session.launch("host-1", "app-1").unwrap();

        let error = session.launch("host-2", "app-2").unwrap_err();

        assert_eq!("A stream session is already running.", error.to_string());
    }

    #[test]
    fn session_event_uses_existing_event_contract() {
        let mut session = SessionMachine::default();
        session.launch("host-1", "app-1").unwrap();
        session.mark_active().unwrap();

        let event = session.state().event();

        assert_eq!(BridgeEventKind::SessionChanged, event.kind);
        assert_eq!("Stream session active.", event.message);
        assert_eq!(Some("host-1".into()), event.host_id);
        assert_eq!(Some("app-1".into()), event.app_id);
    }

    #[test]
    fn finished_session_can_launch_again() {
        let mut session = SessionMachine::default();
        session.launch("host-1", "app-1").unwrap();
        session.finish();

        session.launch("host-2", "app-2").unwrap();

        assert!(matches!(
            session.state(),
            SessionState::Launching { host_id, app_id }
                if host_id == "host-2" && app_id == "app-2"
        ));
    }
}
