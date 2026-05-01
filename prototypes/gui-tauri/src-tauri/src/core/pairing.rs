#![allow(dead_code)]

use super::error::CoreError;
use super::host_http::HostEndpoint;
use aes::cipher::{block_padding::NoPadding, BlockDecryptMut, BlockEncryptMut, KeyInit};

type Aes128EcbEncryptor = ecb::Encryptor<aes::Aes128>;
type Aes128EcbDecryptor = ecb::Decryptor<aes::Aes128>;

const AES_KEY_LENGTH: usize = 16;
const CHALLENGE_LENGTH: usize = 16;
const SHA1_LENGTH: usize = 20;
const SHA256_LENGTH: usize = 32;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PairingHashAlgorithm {
    Sha1,
    Sha256,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerChallenge {
    pub server_response: Vec<u8>,
    pub server_challenge: Vec<u8>,
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

impl PairingHashAlgorithm {
    pub fn for_app_version(app_version: &str) -> Result<Self, CoreError> {
        let major = app_version
            .split('.')
            .next()
            .unwrap_or_default()
            .parse::<u32>()
            .map_err(|_| {
                CoreError::Validation(format!(
                    "Host app version '{app_version}' does not start with a numeric generation."
                ))
            })?;

        if major >= 7 {
            Ok(Self::Sha256)
        } else {
            Ok(Self::Sha1)
        }
    }

    fn hash(self, data: &[u8]) -> Vec<u8> {
        match self {
            Self::Sha1 => {
                use sha1::Digest;
                sha1::Sha1::digest(data).to_vec()
            }
            Self::Sha256 => {
                use sha2::Digest;
                sha2::Sha256::digest(data).to_vec()
            }
        }
    }

    fn response_length(self) -> usize {
        match self {
            Self::Sha1 => SHA1_LENGTH,
            Self::Sha256 => SHA256_LENGTH,
        }
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

pub fn derive_aes_key(
    salt: &[u8],
    pin: &str,
    algorithm: PairingHashAlgorithm,
) -> Result<[u8; AES_KEY_LENGTH], CoreError> {
    if salt.len() != CHALLENGE_LENGTH {
        return Err(CoreError::Validation(
            "Pairing salt must be exactly 16 bytes.".into(),
        ));
    }
    if pin.len() != 4 || !pin.chars().all(|character| character.is_ascii_digit()) {
        return Err(CoreError::Validation(
            "Pairing PIN must be exactly four digits.".into(),
        ));
    }

    let mut salted_pin = Vec::with_capacity(salt.len() + pin.len());
    salted_pin.extend_from_slice(salt);
    salted_pin.extend_from_slice(pin.as_bytes());

    let digest = algorithm.hash(&salted_pin);
    let mut key = [0_u8; AES_KEY_LENGTH];
    key.copy_from_slice(&digest[..AES_KEY_LENGTH]);
    Ok(key)
}

pub fn encrypt_client_challenge(
    random_challenge: &[u8],
    aes_key: &[u8; AES_KEY_LENGTH],
) -> Result<Vec<u8>, CoreError> {
    if random_challenge.len() != CHALLENGE_LENGTH {
        return Err(CoreError::Validation(
            "Pairing client challenge must be exactly 16 bytes.".into(),
        ));
    }
    aes_128_ecb_encrypt(random_challenge, aes_key)
}

pub fn decrypt_server_challenge(
    encrypted_response: &[u8],
    aes_key: &[u8; AES_KEY_LENGTH],
    algorithm: PairingHashAlgorithm,
) -> Result<ServerChallenge, CoreError> {
    let minimum_len = algorithm.response_length() + CHALLENGE_LENGTH;
    if encrypted_response.len() < minimum_len {
        return Err(CoreError::Validation(
            "Pairing challenge response is shorter than expected.".into(),
        ));
    }

    let decrypted = aes_128_ecb_decrypt(encrypted_response, aes_key)?;
    if decrypted.len() < minimum_len {
        return Err(CoreError::Validation(
            "Decrypted pairing challenge response is shorter than expected.".into(),
        ));
    }

    Ok(ServerChallenge {
        server_response: decrypted[..algorithm.response_length()].to_vec(),
        server_challenge: decrypted[algorithm.response_length()..minimum_len].to_vec(),
    })
}

pub fn encrypt_server_challenge_response_hash(
    server_challenge: &[u8],
    client_cert_signature: &[u8],
    client_secret: &[u8],
    aes_key: &[u8; AES_KEY_LENGTH],
    algorithm: PairingHashAlgorithm,
) -> Result<Vec<u8>, CoreError> {
    if server_challenge.len() != CHALLENGE_LENGTH || client_secret.len() != CHALLENGE_LENGTH {
        return Err(CoreError::Validation(
            "Pairing challenge and client secret must be exactly 16 bytes.".into(),
        ));
    }

    let mut challenge_response = Vec::with_capacity(
        server_challenge.len() + client_cert_signature.len() + client_secret.len(),
    );
    challenge_response.extend_from_slice(server_challenge);
    challenge_response.extend_from_slice(client_cert_signature);
    challenge_response.extend_from_slice(client_secret);

    let mut padded_hash = algorithm.hash(&challenge_response);
    padded_hash.resize(SHA256_LENGTH, 0);
    aes_128_ecb_encrypt(&padded_hash, aes_key)
}

pub fn expected_server_response(
    random_challenge: &[u8],
    server_cert_signature: &[u8],
    server_secret: &[u8],
    algorithm: PairingHashAlgorithm,
) -> Result<Vec<u8>, CoreError> {
    if random_challenge.len() != CHALLENGE_LENGTH || server_secret.len() != CHALLENGE_LENGTH {
        return Err(CoreError::Validation(
            "Pairing challenge and server secret must be exactly 16 bytes.".into(),
        ));
    }

    let mut response_data = Vec::with_capacity(
        random_challenge.len() + server_cert_signature.len() + server_secret.len(),
    );
    response_data.extend_from_slice(random_challenge);
    response_data.extend_from_slice(server_cert_signature);
    response_data.extend_from_slice(server_secret);
    Ok(algorithm.hash(&response_data))
}

pub fn client_pairing_secret(client_secret: &[u8], signature: &[u8]) -> Result<Vec<u8>, CoreError> {
    if client_secret.len() != CHALLENGE_LENGTH {
        return Err(CoreError::Validation(
            "Pairing client secret must be exactly 16 bytes.".into(),
        ));
    }

    let mut pairing_secret = Vec::with_capacity(client_secret.len() + signature.len());
    pairing_secret.extend_from_slice(client_secret);
    pairing_secret.extend_from_slice(signature);
    Ok(pairing_secret)
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

pub fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

pub fn decode_hex(label: &'static str, value: &str) -> Result<Vec<u8>, CoreError> {
    if value.len() % 2 != 0 {
        return Err(CoreError::Validation(format!(
            "Pairing {label} must be an even-length hexadecimal string."
        )));
    }

    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = hex_value(pair[0]).ok_or_else(|| invalid_hex_error(label))?;
            let low = hex_value(pair[1]).ok_or_else(|| invalid_hex_error(label))?;
            Ok((high << 4) | low)
        })
        .collect()
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

fn aes_128_ecb_encrypt(
    plaintext: &[u8],
    aes_key: &[u8; AES_KEY_LENGTH],
) -> Result<Vec<u8>, CoreError> {
    if plaintext.is_empty() || plaintext.len() % AES_KEY_LENGTH != 0 {
        return Err(CoreError::Validation(
            "Pairing AES input must be a non-empty multiple of 16 bytes.".into(),
        ));
    }

    Ok(Aes128EcbEncryptor::new(aes_key.into()).encrypt_padded_vec_mut::<NoPadding>(plaintext))
}

fn aes_128_ecb_decrypt(
    ciphertext: &[u8],
    aes_key: &[u8; AES_KEY_LENGTH],
) -> Result<Vec<u8>, CoreError> {
    if ciphertext.is_empty() || ciphertext.len() % AES_KEY_LENGTH != 0 {
        return Err(CoreError::Validation(
            "Pairing AES input must be a non-empty multiple of 16 bytes.".into(),
        ));
    }

    Aes128EcbDecryptor::new(aes_key.into())
        .decrypt_padded_vec_mut::<NoPadding>(ciphertext)
        .map_err(|_| CoreError::Backend("Unable to decrypt pairing payload.".into()))
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn invalid_hex_error(label: &'static str) -> CoreError {
    CoreError::Validation(format!(
        "Pairing {label} must be an even-length hexadecimal string."
    ))
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
        client_pairing_secret, decode_hex, derive_aes_key, encode_hex, encrypt_client_challenge,
        encrypt_server_challenge_response_hash, expected_server_response, parse_pairing_response,
        parse_pairing_state, PairingHashAlgorithm, PairingHttpSequence, PairingMaterial,
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

    #[test]
    fn app_version_selects_native_hash_generation() {
        assert_eq!(
            PairingHashAlgorithm::Sha1,
            PairingHashAlgorithm::for_app_version("6.1.0").unwrap()
        );
        assert_eq!(
            PairingHashAlgorithm::Sha256,
            PairingHashAlgorithm::for_app_version("7.1.431").unwrap()
        );
    }

    #[test]
    fn aes_key_derivation_truncates_native_pin_hash() {
        let salt = decode_hex("salt", "000102030405060708090a0b0c0d0e0f").unwrap();
        let key = derive_aes_key(&salt, "1234", PairingHashAlgorithm::Sha256).unwrap();

        assert_eq!("bad0b4f7cae08eb7c1b5acc763a8ed25", encode_hex(&key));
    }

    #[test]
    fn client_challenge_uses_aes_128_ecb_without_padding() {
        let key: [u8; 16] = decode_hex("key", "000102030405060708090a0b0c0d0e0f")
            .unwrap()
            .try_into()
            .unwrap();
        let challenge = decode_hex("challenge", "00112233445566778899aabbccddeeff").unwrap();

        let encrypted = encrypt_client_challenge(&challenge, &key).unwrap();

        assert_eq!("69c4e0d86a7b0430d8cdb78070b4c55a", encode_hex(&encrypted));
    }

    #[test]
    fn server_challenge_hash_is_padded_and_encrypted_for_sha1() {
        let key: [u8; 16] = decode_hex("key", "000102030405060708090a0b0c0d0e0f")
            .unwrap()
            .try_into()
            .unwrap();
        let server_challenge =
            decode_hex("server challenge", "101112131415161718191a1b1c1d1e1f").unwrap();
        let client_secret =
            decode_hex("client secret", "202122232425262728292a2b2c2d2e2f").unwrap();

        let encrypted = encrypt_server_challenge_response_hash(
            &server_challenge,
            b"client-cert-signature",
            &client_secret,
            &key,
            PairingHashAlgorithm::Sha1,
        )
        .unwrap();

        assert_eq!(32, encrypted.len());
    }

    #[test]
    fn expected_server_response_and_client_secret_match_native_ordering() {
        let random_challenge = decode_hex("challenge", "000102030405060708090a0b0c0d0e0f").unwrap();
        let server_secret =
            decode_hex("server secret", "101112131415161718191a1b1c1d1e1f").unwrap();

        let expected = expected_server_response(
            &random_challenge,
            b"server-cert-signature",
            &server_secret,
            PairingHashAlgorithm::Sha256,
        )
        .unwrap();
        let pairing_secret = client_pairing_secret(&server_secret, b"client-signature").unwrap();

        assert_eq!(32, expected.len());
        assert_eq!(
            "101112131415161718191a1b1c1d1e1f636c69656e742d7369676e6174757265",
            encode_hex(&pairing_secret)
        );
    }
}
