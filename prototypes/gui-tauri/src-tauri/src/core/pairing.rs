#![allow(dead_code)]

use super::error::CoreError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PairingState {
    Paired,
    PinWrong,
    Failed(String),
    AlreadyInProgress,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingRequest {
    pub host_id: String,
    pub pin: String,
    pub app_version: String,
}

impl PairingRequest {
    pub fn new(
        host_id: impl Into<String>,
        pin: impl Into<String>,
        app_version: impl Into<String>,
    ) -> Result<Self, CoreError> {
        let request = Self {
            host_id: host_id.into(),
            pin: pin.into(),
            app_version: app_version.into(),
        };
        request.validate()?;
        Ok(request)
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        if self.host_id.trim().is_empty() {
            return Err(CoreError::Validation("Host ID is required.".into()));
        }
        if self.pin.len() != 4 || !self.pin.chars().all(|character| character.is_ascii_digit()) {
            return Err(CoreError::Validation(
                "Pairing PIN must be exactly four digits.".into(),
            ));
        }
        if self.app_version.trim().is_empty() {
            return Err(CoreError::Validation(
                "Host app version is required for pairing.".into(),
            ));
        }
        Ok(())
    }
}

pub fn parse_pairing_state(pair_status: &str, error_message: Option<&str>) -> PairingState {
    match pair_status.trim() {
        "1" | "paired" | "Paired" => PairingState::Paired,
        "401" | "pin_wrong" | "PinWrong" => PairingState::PinWrong,
        "in_progress" | "AlreadyInProgress" => PairingState::AlreadyInProgress,
        _ => PairingState::Failed(
            error_message
                .filter(|message| !message.trim().is_empty())
                .unwrap_or("Pairing failed.")
                .to_string(),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_pairing_state, PairingRequest, PairingState};

    #[test]
    fn pairing_request_requires_four_digit_pin() {
        let error = PairingRequest::new("gaming-pc", "12ab", "Sunshine").unwrap_err();

        assert_eq!(
            "Pairing PIN must be exactly four digits.",
            error.to_string()
        );
    }

    #[test]
    fn pairing_request_accepts_valid_input() {
        let request = PairingRequest::new("gaming-pc", "1234", "Sunshine").unwrap();

        assert_eq!("gaming-pc", request.host_id);
        assert_eq!("1234", request.pin);
    }

    #[test]
    fn pairing_state_parser_maps_known_states() {
        assert_eq!(PairingState::Paired, parse_pairing_state("1", None));
        assert_eq!(PairingState::PinWrong, parse_pairing_state("401", None));
        assert_eq!(
            PairingState::AlreadyInProgress,
            parse_pairing_state("in_progress", None)
        );
    }

    #[test]
    fn pairing_state_parser_preserves_failure_message() {
        assert_eq!(
            PairingState::Failed("Certificate rejected.".into()),
            parse_pairing_state("500", Some("Certificate rejected."))
        );
    }
}
