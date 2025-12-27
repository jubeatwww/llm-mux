use actix_web::{HttpResponse, ResponseError};
use serde::Serialize;
use std::fmt;

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
}

#[derive(Debug)]
pub enum AppError {
    ProviderExecution { message: String, stderr: String },
    ProviderNotFound(String),
    ModelNotFound { provider: String, model: String },
    RateLimited { provider: String, model: String },
    Timeout { provider: String, timeout_secs: u64 },
    InvalidSchema(String),
    ConfigLoad(String),
    OutputParse { message: String, stdout: String },
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProviderExecution { message, .. } => {
                write!(f, "provider execution failed: {message}")
            }
            Self::ProviderNotFound(p) => write!(f, "provider not found: {p}"),
            Self::ModelNotFound { provider, model } => {
                write!(f, "model '{model}' not found for provider '{provider}'")
            }
            Self::RateLimited { provider, model } => write!(f, "rate limited: {provider}/{model}"),
            Self::Timeout { provider, timeout_secs } => {
                write!(f, "{provider} timed out after {timeout_secs}s")
            }
            Self::InvalidSchema(e) => write!(f, "invalid schema: {e}"),
            Self::ConfigLoad(e) => write!(f, "config load error: {e}"),
            Self::OutputParse { message, .. } => write!(f, "output parse error: {message}"),
        }
    }
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
            Self::ProviderNotFound(_) | Self::ModelNotFound { .. } => (
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
        };
        HttpResponse::build(status).json(response)
    }
}
