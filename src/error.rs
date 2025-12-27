use actix_web::{HttpResponse, ResponseError};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
}

#[derive(Debug, Error)]
pub enum AppError {
    #[error("provider execution failed: {message}")]
    ProviderExecution { message: String, stderr: String },

    #[error("provider not found: {0}")]
    ProviderNotFound(String),

    #[error("model '{model:?}' not found for provider '{provider}'")]
    ModelNotFound {
        provider: String,
        model: Option<String>,
    },

    #[error("rate limited: {provider}/{model:?}")]
    RateLimited {
        provider: String,
        model: Option<String>,
    },

    #[error("provider '{0}' does not support auto model selection")]
    AutoModelNotSupported(String),

    #[error("{provider} timed out after {timeout_secs}s")]
    Timeout { provider: String, timeout_secs: u64 },

    #[error("invalid schema: {0}")]
    InvalidSchema(String),

    #[error("config load error: {0}")]
    ConfigLoad(String),

    #[error("output parse error: {message}")]
    OutputParse { message: String, stdout: String },

    #[error("output validation failed: {errors:?}")]
    OutputValidation {
        errors: Vec<String>,
        output: serde_json::Value,
    },
}

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        let (status, response) = match self {
            Self::ProviderExecution { message, stderr } => (
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                ErrorResponse {
                    error: message.clone(),
                    stderr: Some(stderr.clone()),
                },
            ),
            Self::ProviderNotFound(_)
            | Self::ModelNotFound { .. }
            | Self::AutoModelNotSupported(_) => (
                actix_web::http::StatusCode::BAD_REQUEST,
                ErrorResponse {
                    error: self.to_string(),
                    stderr: None,
                },
            ),
            Self::RateLimited { .. } => (
                actix_web::http::StatusCode::TOO_MANY_REQUESTS,
                ErrorResponse {
                    error: self.to_string(),
                    stderr: None,
                },
            ),
            Self::Timeout { .. } => (
                actix_web::http::StatusCode::GATEWAY_TIMEOUT,
                ErrorResponse {
                    error: self.to_string(),
                    stderr: None,
                },
            ),
            Self::InvalidSchema(_) => (
                actix_web::http::StatusCode::BAD_REQUEST,
                ErrorResponse {
                    error: self.to_string(),
                    stderr: None,
                },
            ),
            Self::ConfigLoad(_) => (
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                ErrorResponse {
                    error: self.to_string(),
                    stderr: None,
                },
            ),
            Self::OutputParse { message, stdout } => (
                actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
                ErrorResponse {
                    error: message.clone(),
                    stderr: Some(stdout.clone()),
                },
            ),
            Self::OutputValidation { errors, output } => (
                actix_web::http::StatusCode::UNPROCESSABLE_ENTITY,
                ErrorResponse {
                    error: format!("output validation failed: {}", errors.join("; ")),
                    stderr: Some(output.to_string()),
                },
            ),
        };
        HttpResponse::build(status).json(response)
    }
}
