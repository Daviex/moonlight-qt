#![allow(dead_code)]

use super::error::CoreError;
use super::host_http::HostEndpoint;

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingResponse {
    pub paired: bool,
    pub plain_cert_hex: String,
    pub challenge_response_hex: String,
    pub pairing_secret_hex: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingHttpSequence {
    pub get_server_cert_url: String,
    pub client_challenge_url: String,
    pub server_challenge_response_url: String,
    pub client_pairing_secret_url: String,
    pub pair_challenge_url: String,
    pub unpair_url: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PairingMaterial {
    pub salt_hex: String,
    pub client_cert_hex: String,
    pub encrypted_challenge_hex: String,
    pub encrypted_server_challenge_response_hex: String,
    pub client_pairing_secret_hex: String,
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

impl PairingMaterial {
    pub fn validate(&self) -> Result<(), CoreError> {
        validate_hex("salt", &self.salt_hex)?;
        validate_hex("client certificate", &self.client_cert_hex)?;
        validate_hex("encrypted challenge", &self.encrypted_challenge_hex)?;
        validate_hex(
            "encrypted server challenge response",
            &self.encrypted_server_challenge_response_hex,
        )?;
        validate_hex("client pairing secret", &self.client_pairing_secret_hex)?;
        Ok(())
    }
}

impl PairingHttpSequence {
    pub fn new(endpoint: &HostEndpoint, material: &PairingMaterial) -> Result<Self, CoreError> {
        material.validate()?;
        Ok(Self {
            get_server_cert_url: endpoint.http_pair_url(&format!(
                "devicename=roth&updateState=1&phrase=getservercert&salt={}&clientcert={}",
                material.salt_hex, material.client_cert_hex
            )),
            client_challenge_url: endpoint.http_pair_url(&format!(
                "devicename=roth&updateState=1&clientchallenge={}",
                material.encrypted_challenge_hex
            )),
            server_challenge_response_url: endpoint.http_pair_url(&format!(
                "devicename=roth&updateState=1&serverchallengeresp={}",
                material.encrypted_server_challenge_response_hex
            )),
            client_pairing_secret_url: endpoint.http_pair_url(&format!(
                "devicename=roth&updateState=1&clientpairingsecret={}",
                material.client_pairing_secret_hex
            )),
            pair_challenge_url: endpoint
                .https_pair_url("devicename=roth&updateState=1&phrase=pairchallenge"),
            unpair_url: endpoint.http_unpair_url(),
        })
    }
}

pub fn parse_pairing_response(xml: &str) -> PairingResponse {
    PairingResponse {
        paired: optional_tag(xml, "paired") == "1",
        plain_cert_hex: optional_tag(xml, "plaincert"),
        challenge_response_hex: optional_tag(xml, "challengeresponse"),
        pairing_secret_hex: optional_tag(xml, "pairingsecret"),
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

fn validate_hex(label: &'static str, value: &str) -> Result<(), CoreError> {
    if value.is_empty()
        || value.len() % 2 != 0
        || !value.chars().all(|character| character.is_ascii_hexdigit())
    {
        return Err(CoreError::Validation(format!(
            "Pairing {label} must be an even-length hexadecimal string."
        )));
    }
    Ok(())
}

fn optional_tag(xml: &str, tag: &str) -> String {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let Some(start) = xml.find(&open) else {
        return String::new();
    };
    let content_start = start + open.len();
    let Some(relative_end) = xml[content_start..].find(&close) else {
        return String::new();
    };

    xml[content_start..content_start + relative_end]
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::{
        parse_pairing_response, parse_pairing_state, PairingHttpSequence, PairingMaterial,
        PairingRequest, PairingState,
    };
    use crate::core::host_http::HostEndpoint;

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

    #[test]
    fn pairing_sequence_builds_native_stage_urls() {
        let endpoint = HostEndpoint::from_address("192.168.1.20").unwrap();
        let material = PairingMaterial {
            salt_hex: "00112233445566778899aabbccddeeff".into(),
            client_cert_hex: "aabbccdd".into(),
            encrypted_challenge_hex: "11223344".into(),
            encrypted_server_challenge_response_hex: "55667788".into(),
            client_pairing_secret_hex: "99aabbcc".into(),
        };

        let sequence = PairingHttpSequence::new(&endpoint, &material).unwrap();

        assert_eq!(
            "http://192.168.1.20:47989/pair?devicename=roth&updateState=1&phrase=getservercert&salt=00112233445566778899aabbccddeeff&clientcert=aabbccdd",
            sequence.get_server_cert_url
        );
        assert_eq!(
            "https://192.168.1.20:47984/pair?devicename=roth&updateState=1&phrase=pairchallenge",
            sequence.pair_challenge_url
        );
    }

    #[test]
    fn pairing_sequence_rejects_invalid_hex_material() {
        let endpoint = HostEndpoint::from_address("192.168.1.20").unwrap();
        let material = PairingMaterial {
            salt_hex: "not-hex".into(),
            client_cert_hex: "aabbccdd".into(),
            encrypted_challenge_hex: "11223344".into(),
            encrypted_server_challenge_response_hex: "55667788".into(),
            client_pairing_secret_hex: "99aabbcc".into(),
        };

        let error = PairingHttpSequence::new(&endpoint, &material).unwrap_err();

        assert_eq!(
            "Pairing salt must be an even-length hexadecimal string.",
            error.to_string()
        );
    }

    #[test]
    fn pairing_response_parser_extracts_stage_fields() {
        let response = parse_pairing_response(
            "<root><paired>1</paired><plaincert>aabb</plaincert><challengeresponse>ccdd</challengeresponse><pairingsecret>eeff</pairingsecret></root>",
        );

        assert!(response.paired);
        assert_eq!("aabb", response.plain_cert_hex);
        assert_eq!("ccdd", response.challenge_response_hex);
        assert_eq!("eeff", response.pairing_secret_hex);
    }
}
