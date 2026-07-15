use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: &'static str,
    pub message: String,
    pub correlation_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("validation failed")]
    Validation,
    #[error("resource not found")]
    NotFound,
    #[error("file operation failed")]
    Io(#[from] std::io::Error),
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
    #[error("secret store operation failed")]
    SecretStore,
    #[error("blueprint schema version is not supported")]
    UnsupportedVersion,
    #[error("database capability denied")]
    CapabilityDenied,
    #[error("path is invalid")]
    InvalidPath,
    #[error("internal operation failed")]
    Internal,
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Validation => "VALIDATION_ERROR",
            Self::NotFound => "NOT_FOUND",
            Self::Io(_) => "IO_ERROR",
            Self::Database(_) => "DATABASE_ERROR",
            Self::SecretStore => "SECRET_STORE_ERROR",
            Self::UnsupportedVersion => "UNSUPPORTED_VERSION",
            Self::CapabilityDenied => "CAPABILITY_DENIED",
            Self::InvalidPath => "INVALID_PATH",
            Self::Internal => "INTERNAL_ERROR",
        }
    }

    pub fn command_error(&self) -> CommandError {
        CommandError {
            code: self.code(),
            message: self.to_string(),
            correlation_id: Uuid::new_v4().to_string(),
            details: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandResponse<T: Serialize> {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<CommandError>,
}

impl<T: Serialize> CommandResponse<T> {
    pub fn from_result(result: Result<T, AppError>) -> Self {
        match result {
            Ok(data) => Self {
                ok: true,
                data: Some(data),
                error: None,
            },
            Err(error) => Self {
                ok: false,
                data: None,
                error: Some(error.command_error()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_boolean_success_and_error_envelopes() {
        let success = serde_json::to_value(CommandResponse::from_result(Ok("ready"))).unwrap();
        assert_eq!(success["ok"], true);
        assert_eq!(success["data"], "ready");
        assert!(success.get("error").is_none());

        let failure = serde_json::to_value(CommandResponse::<()>::from_result(Err(
            AppError::CapabilityDenied,
        )))
        .unwrap();
        assert_eq!(failure["ok"], false);
        assert_eq!(failure["error"]["code"], "CAPABILITY_DENIED");
        assert!(failure["error"]["correlationId"].is_string());
        assert!(failure.get("data").is_none());
    }
}
