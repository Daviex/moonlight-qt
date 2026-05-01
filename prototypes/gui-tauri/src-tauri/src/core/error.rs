use std::error::Error;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreError {
    Validation(String),
    NotFound { entity: &'static str, id: String },
    Backend(String),
}

impl fmt::Display for CoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(message) | Self::Backend(message) => formatter.write_str(message),
            Self::NotFound { entity, id } => write!(formatter, "{entity} '{id}' was not found."),
        }
    }
}

impl Error for CoreError {}

impl From<CoreError> for String {
    fn from(error: CoreError) -> Self {
        error.to_string()
    }
}

impl From<String> for CoreError {
    fn from(error: String) -> Self {
        Self::Backend(error)
    }
}

impl From<&str> for CoreError {
    fn from(error: &str) -> Self {
        Self::Backend(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::CoreError;

    #[test]
    fn not_found_error_formats_user_message() {
        let error = CoreError::NotFound {
            entity: "Host",
            id: "living-room".into(),
        };

        assert_eq!("Host 'living-room' was not found.", error.to_string());
    }

    #[test]
    fn validation_error_preserves_message() {
        let error = CoreError::Validation("Width must be at least 256.".into());

        assert_eq!("Width must be at least 256.", String::from(error));
    }
}
