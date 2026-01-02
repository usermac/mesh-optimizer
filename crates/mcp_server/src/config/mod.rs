//! Configuration module for MeshOpt MCP Server
//!
//! Handles environment variables and configuration settings.

use std::env;
use thiserror::Error;

/// Default API URL for MeshOpt
pub const DEFAULT_API_URL: &str = "https://api.webdeliveryengine.com";

/// Configuration errors
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Missing required environment variable: {0}")]
    MissingEnvVar(String),

    #[error("Invalid configuration: {0}")]
    Invalid(String),
}

/// MCP Server configuration loaded from environment
#[derive(Debug, Clone)]
pub struct Config {
    /// MeshOpt API key (required)
    pub api_key: String,

    /// MeshOpt API base URL (optional, defaults to production)
    pub api_url: String,

    /// Enable debug logging
    pub debug: bool,
}

impl Config {
    /// Load configuration from environment variables
    ///
    /// Required:
    /// - `MESHOPT_API_KEY`: Your MeshOpt API key
    ///
    /// Optional:
    /// - `MESHOPT_API_URL`: Override the API URL (default: https://api.webdeliveryengine.com)
    /// - `MESHOPT_DEBUG`: Enable debug logging (default: false)
    pub fn from_env() -> Result<Self, ConfigError> {
        let api_key = env::var("MESHOPT_API_KEY")
            .map_err(|_| ConfigError::MissingEnvVar("MESHOPT_API_KEY".to_string()))?;

        if api_key.is_empty() {
            return Err(ConfigError::Invalid(
                "MESHOPT_API_KEY cannot be empty".to_string(),
            ));
        }

        let api_url = env::var("MESHOPT_API_URL").unwrap_or_else(|_| DEFAULT_API_URL.to_string());

        let debug = env::var("MESHOPT_DEBUG")
            .map(|v| v == "1" || v.to_lowercase() == "true")
            .unwrap_or(false);

        Ok(Config {
            api_key,
            api_url,
            debug,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_api_url() {
        assert_eq!(DEFAULT_API_URL, "https://api.webdeliveryengine.com");
    }
}
