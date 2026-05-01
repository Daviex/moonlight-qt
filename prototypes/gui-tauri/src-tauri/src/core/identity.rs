#![allow(dead_code)]

use super::error::CoreError;
use rand::rngs::OsRng;
use rand::RngCore;
use rsa::pkcs1v15::{Signature as RsaSignature, SigningKey, VerifyingKey};
use rsa::pkcs8::{
    DecodePrivateKey, DecodePublicKey, EncodePrivateKey, EncodePublicKey, LineEnding,
};
use rsa::signature::{RandomizedSigner, SignatureEncoding, Verifier};
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::str::FromStr;
use std::time::Duration;
use x509_cert::builder::{Builder, CertificateBuilder, Profile};
use x509_cert::der::{DecodePem, Encode, EncodePem};
use x509_cert::name::Name;
use x509_cert::serial_number::SerialNumber;
use x509_cert::spki::SubjectPublicKeyInfoOwned;
use x509_cert::time::Validity;
use x509_parser::parse_x509_certificate;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientIdentity {
    pub unique_id: String,
    pub certificate_pem: String,
    pub private_key_pem: String,
}

impl ClientIdentity {
    pub fn generate() -> Result<Self, CoreError> {
        let mut rng = OsRng;
        let mut unique_id = [0_u8; 8];
        rng.fill_bytes(&mut unique_id);

        let private_key = RsaPrivateKey::new(&mut rng, 2048)
            .map_err(|error| CoreError::Backend(format!("Unable to generate RSA key: {error}")))?;
        let public_key = RsaPublicKey::from(&private_key);
        let certificate_pem = generate_self_signed_certificate_pem(&private_key, &public_key)?;
        let private_key_pem = private_key
            .to_pkcs8_pem(LineEnding::LF)
            .map_err(|error| {
                CoreError::Backend(format!("Unable to encode identity private key: {error}"))
            })?
            .to_string();

        Self::new(encode_hex(&unique_id), certificate_pem, private_key_pem)
    }

    pub fn new(
        unique_id: impl Into<String>,
        certificate_pem: impl Into<String>,
        private_key_pem: impl Into<String>,
    ) -> Result<Self, CoreError> {
        let identity = Self {
            unique_id: unique_id.into(),
            certificate_pem: certificate_pem.into(),
            private_key_pem: private_key_pem.into(),
        };
        identity.validate()?;
        Ok(identity)
    }

    pub fn validate(&self) -> Result<(), CoreError> {
        if self.unique_id.trim().is_empty() {
            return Err(CoreError::Validation(
                "Client identity unique ID is required.".into(),
            ));
        }
        if !self.certificate_pem.contains("BEGIN CERTIFICATE") {
            return Err(CoreError::Validation(
                "Client identity certificate must be PEM encoded.".into(),
            ));
        }
        if !self.private_key_pem.contains("BEGIN") || !self.private_key_pem.contains("PRIVATE KEY")
        {
            return Err(CoreError::Validation(
                "Client identity private key must be PEM encoded.".into(),
            ));
        }
        Ok(())
    }

    pub fn certificate_signature(&self) -> Result<Vec<u8>, CoreError> {
        certificate_signature(&self.certificate_pem)
    }

    pub fn sign_message(&self, message: &[u8]) -> Result<Vec<u8>, CoreError> {
        let private_key =
            RsaPrivateKey::from_pkcs8_pem(&self.private_key_pem).map_err(|error| {
                CoreError::Validation(format!(
                    "Client identity private key is unreadable: {error}"
                ))
            })?;
        let signing_key = SigningKey::<Sha256>::new(private_key);
        let mut rng = OsRng;
        Ok(signing_key.sign_with_rng(&mut rng, message).to_vec())
    }
}

pub fn certificate_signature(certificate_pem: &str) -> Result<Vec<u8>, CoreError> {
    let certificate = x509_cert::Certificate::from_pem(certificate_pem).map_err(|error| {
        CoreError::Validation(format!(
            "Client identity certificate is unreadable: {error}"
        ))
    })?;
    Ok(certificate.signature.raw_bytes().to_vec())
}

pub fn verify_certificate_signature(
    message: &[u8],
    signature: &[u8],
    certificate_pem: &str,
) -> Result<bool, CoreError> {
    let public_key = public_key_from_certificate(certificate_pem)?;
    let verifying_key = VerifyingKey::<Sha256>::new(public_key);
    let signature = RsaSignature::try_from(signature).map_err(|error| {
        CoreError::Validation(format!("Pairing signature is malformed: {error}"))
    })?;

    Ok(verifying_key.verify(message, &signature).is_ok())
}

fn generate_self_signed_certificate_pem(
    private_key: &RsaPrivateKey,
    public_key: &RsaPublicKey,
) -> Result<String, CoreError> {
    let public_key_der = public_key.to_public_key_der().map_err(|error| {
        CoreError::Backend(format!("Unable to encode identity public key: {error}"))
    })?;
    let subject_public_key_info = SubjectPublicKeyInfoOwned::try_from(public_key_der.as_ref())
        .map_err(|error| CoreError::Backend(format!("Unable to build identity SPKI: {error}")))?;
    let subject = Name::from_str("CN=NVIDIA GameStream Client").map_err(|error| {
        CoreError::Backend(format!("Unable to build identity subject name: {error}"))
    })?;
    let validity =
        Validity::from_now(Duration::from_secs(60 * 60 * 24 * 365 * 20)).map_err(|error| {
            CoreError::Backend(format!("Unable to build identity validity: {error}"))
        })?;
    let signing_key = SigningKey::<Sha256>::new(private_key.clone());
    let builder = CertificateBuilder::new(
        Profile::Root,
        SerialNumber::from(0_u32),
        validity,
        subject,
        subject_public_key_info,
        &signing_key,
    )
    .map_err(|error| {
        CoreError::Backend(format!("Unable to build identity certificate: {error}"))
    })?;

    builder
        .build::<RsaSignature>()
        .map_err(|error| {
            CoreError::Backend(format!("Unable to sign identity certificate: {error}"))
        })?
        .to_pem(LineEnding::LF)
        .map_err(|error| {
            CoreError::Backend(format!("Unable to encode identity certificate: {error}"))
        })
}

fn public_key_from_certificate(certificate_pem: &str) -> Result<RsaPublicKey, CoreError> {
    let certificate_der = x509_cert::Certificate::from_pem(certificate_pem)
        .and_then(|certificate| certificate.to_der())
        .map_err(|error| {
            CoreError::Validation(format!("Pairing certificate is unreadable: {error}"))
        })?;
    let (_, parsed) = parse_x509_certificate(&certificate_der).map_err(|error| {
        CoreError::Validation(format!("Pairing certificate is unreadable: {error}"))
    })?;

    RsaPublicKey::from_public_key_der(parsed.public_key().raw).map_err(|error| {
        CoreError::Validation(format!(
            "Pairing certificate RSA public key is unreadable: {error}"
        ))
    })
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::{verify_certificate_signature, ClientIdentity};

    #[test]
    fn identity_requires_unique_id() {
        let error = ClientIdentity::new(
            "",
            "-----BEGIN CERTIFICATE-----\n-----END CERTIFICATE-----",
            "-----BEGIN PRIVATE KEY-----\n-----END PRIVATE KEY-----",
        )
        .unwrap_err();

        assert_eq!("Client identity unique ID is required.", error.to_string());
    }

    #[test]
    fn identity_serializes_with_camel_case_fields() {
        let identity = ClientIdentity::new(
            "abc123",
            "-----BEGIN CERTIFICATE-----\n-----END CERTIFICATE-----",
            "-----BEGIN PRIVATE KEY-----\n-----END PRIVATE KEY-----",
        )
        .unwrap();

        let value = serde_json::to_value(identity).unwrap();

        assert_eq!("abc123", value["uniqueId"]);
        assert!(value.get("certificatePem").is_some());
        assert!(value.get("privateKeyPem").is_some());
    }

    #[test]
    fn generated_identity_contains_usable_certificate_and_key() {
        let identity = ClientIdentity::generate().unwrap();

        assert_eq!(16, identity.unique_id.len());
        assert!(identity.certificate_pem.contains("BEGIN CERTIFICATE"));
        assert!(identity.private_key_pem.contains("BEGIN PRIVATE KEY"));
        assert!(!identity.certificate_signature().unwrap().is_empty());
    }

    #[test]
    fn identity_signatures_verify_against_own_certificate() {
        let identity = ClientIdentity::generate().unwrap();
        let message = b"pairing secret";

        let signature = identity.sign_message(message).unwrap();

        assert!(
            verify_certificate_signature(message, &signature, &identity.certificate_pem).unwrap()
        );
        assert!(
            !verify_certificate_signature(b"tampered", &signature, &identity.certificate_pem)
                .unwrap()
        );
    }
}
