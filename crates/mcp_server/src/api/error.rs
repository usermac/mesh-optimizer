//! API error types for MeshOpt client

use serde::Deserialize;
use thiserror::Error;

/// Structured error response from MeshOpt API
#[derive(Debug, Deserialize)]
pub struct ApiErrorResponse {
    pub error: String,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub balance: Option<i32>,
    #[serde(default)]
    pub required: Option<i32>,
    #[serde(default)]
    pub message: Option<String>,
}

/// Errors that can occur when interacting with the MeshOpt API
#[derive(Debug, Error)]
pub enum ApiError {
    /// Authentication failed (invalid or missing API key)
    #[error("Unauthorized: {message}")]
    Unauthorized { message: String },

    /// Insufficient credits to perform operation
    #[error("Insufficient credits: need {required}, have {balance}")]
    InsufficientCredits { balance: i32, required: i32 },

    /// Bad request (invalid parameters, unsupported file type, etc.)
    #[error("Bad request: {message}")]
    BadRequest { message: String },

    /// Resource not found
    #[error("Not found: {message}")]
    NotFound { message: String },

    /// Server error
    #[error("Server error: {message}")]
    ServerError { message: String },

    /// Request timeout
    #[error("Request timed out: {message}")]
    Timeout { message: String },

    /// Network or HTTP error
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    /// JSON parsing error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    /// File I/O error
    #[error("File error: {0}")]
    FileError(String),
}

impl ApiError {
    /// Create an ApiError from an HTTP status code and optional error response
    pub fn from_response(status: reqwest::StatusCode, response: Option<ApiErrorResponse>) -> Self {
        match status {
            reqwest::StatusCode::UNAUTHORIZED => ApiError::Unauthorized {
                message: response
                    .and_then(|r| r.message.or(Some(r.error)))
                    .unwrap_or_else(|| "Invalid or missing API key".to_string()),
            },
            reqwest::StatusCode::PAYMENT_REQUIRED => {
                if let Some(r) = response {
                    if r.code.as_deref() == Some("insufficient_credits") {
                        return ApiError::InsufficientCredits {
                            balance: r.balance.unwrap_or(0),
                            required: r.required.unwrap_or(0),
                        };
                    }
                }
                ApiError::InsufficientCredits {
                    balance: 0,
                    required: 0,
                }
            }
            reqwest::StatusCode::BAD_REQUEST => ApiError::BadRequest {
                message: response
                    .map(|r| r.message.unwrap_or(r.error))
                    .unwrap_or_else(|| "Invalid request".to_string()),
            },
            reqwest::StatusCode::NOT_FOUND => ApiError::NotFound {
                message: response
                    .map(|r| r.message.unwrap_or(r.error))
                    .unwrap_or_else(|| "Resource not found".to_string()),
            },
            reqwest::StatusCode::GATEWAY_TIMEOUT | reqwest::StatusCode::REQUEST_TIMEOUT => {
                ApiError::Timeout {
                    message: response
                        .map(|r| r.message.unwrap_or(r.error))
                        .unwrap_or_else(|| "Request timed out".to_string()),
                }
            }
            _ => ApiError::ServerError {
                message: response
                    .map(|r| r.message.unwrap_or(r.error))
                    .unwrap_or_else(|| format!("Server returned status {}", status)),
            },
        }
    }

    /// Get a human-readable message suitable for displaying to users
    pub fn user_message(&self) -> String {
        match self {
            ApiError::Unauthorized { .. } => {
                "Invalid or missing API key. Get your key at webdeliveryengine.com".to_string()
            }
            ApiError::InsufficientCredits { balance, required } => {
                format!(
                    "Insufficient credits: need {} credits, have {}. Purchase more at webdeliveryengine.com",
                    required, balance
                )
            }
            ApiError::BadRequest { message } => message.clone(),
            ApiError::NotFound { message } => message.clone(),
            ApiError::ServerError { message } => {
                format!("Server error: {}. Please try again.", message)
            }
            ApiError::Timeout { message } => message.clone(),
            ApiError::Network(e) => format!("Network error: {}", e),
            ApiError::Json(e) => format!("Failed to parse response: {}", e),
            ApiError::FileError(msg) => msg.clone(),
        }
    }
}
