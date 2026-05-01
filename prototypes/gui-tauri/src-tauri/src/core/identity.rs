#![allow(dead_code)]

use super::error::CoreError;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientIdentity {
    pub unique_id: String,
    pub certificate_pem: String,
    pub private_key_pem: String,
}

impl ClientIdentity {
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
}

#[cfg(test)]
mod tests {
    use super::ClientIdentity;

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
}
