//! HTTP client for MeshOpt API

use crate::api::error::{ApiError, ApiErrorResponse};
use crate::api::types::*;
use crate::config::Config;
use reqwest::multipart::{Form, Part};
use reqwest::Client;
use std::path::Path;
use std::time::Duration;
use tracing::{debug, error};

/// MeshOpt API client
pub struct MeshOptClient {
    client: Client,
    config: Config,
}

impl MeshOptClient {
    /// Create a new API client with the given configuration
    pub fn new(config: Config) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(660)) // 11 minutes (longer than server timeout)
            .build()
            .expect("Failed to create HTTP client");

        Self { client, config }
    }

    /// Get the authorization header value
    fn auth_header(&self) -> String {
        format!("Bearer {}", self.config.api_key)
    }

    /// Build the full URL for an endpoint
    fn url(&self, endpoint: &str) -> String {
        format!("{}{}", self.config.api_url, endpoint)
    }

    /// Handle an error response from the API
    async fn handle_error_response(&self, response: reqwest::Response) -> ApiError {
        let status = response.status();
        let error_response: Option<ApiErrorResponse> = response.json().await.ok();
        ApiError::from_response(status, error_response)
    }

    /// Check credit balance
    pub async fn get_credits(&self) -> Result<i32, ApiError> {
        debug!("Fetching credit balance");

        let response = self
            .client
            .get(self.url("/credits"))
            .header("Authorization", self.auth_header())
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(self.handle_error_response(response).await);
        }

        let credits: CreditsResponse = response.json().await?;
        Ok(credits.credits)
    }

    /// Get usage history
    pub async fn get_history(&self, limit: Option<u32>) -> Result<Vec<HistoryEntry>, ApiError> {
        debug!("Fetching usage history");

        let url = match limit {
            Some(l) => format!("{}?limit={}", self.url("/history"), l),
            None => self.url("/history"),
        };

        let response = self
            .client
            .get(&url)
            .header("Authorization", self.auth_header())
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(self.handle_error_response(response).await);
        }

        let history: HistoryResponse = response.json().await?;
        Ok(history.history)
    }

    /// Optimize a mesh file (blocking mode)
    ///
    /// Uploads the file to the API and waits for processing to complete.
    /// Returns the completed job status with download URLs.
    pub async fn optimize(
        &self,
        file_path: &Path,
        file_data: Vec<u8>,
        params: &OptimizeParams,
    ) -> Result<BlockingOptimizeResponse, ApiError> {
        debug!("Starting optimization for {:?}", file_path);

        let filename = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("model.glb");

        // Determine content type
        let content_type = mime_guess::from_path(file_path)
            .first_or_octet_stream()
            .to_string();

        // Build multipart form
        let file_part = Part::bytes(file_data)
            .file_name(filename.to_string())
            .mime_str(&content_type)
            .map_err(|e| ApiError::FileError(e.to_string()))?;

        let mut form = Form::new().part("file", file_part);

        // Add mode
        form = form.text("mode", params.mode.clone());

        // Add optional parameters based on mode
        if params.mode == "decimate" {
            if let Some(ratio) = params.ratio {
                form = form.text("ratio", ratio.to_string());
            }
        } else if params.mode == "remesh" {
            if let Some(faces) = params.faces {
                form = form.text("faces", faces.to_string());
            }
            if let Some(texture_size) = params.texture_size {
                form = form.text("texture_size", texture_size.to_string());
            }
        }

        // Add output format if specified
        if let Some(ref format) = params.format {
            form = form.text("format", format.clone());
        }

        // Use blocking mode to wait for completion
        let url = format!("{}?blocking=true", self.url("/optimize"));

        debug!("Sending optimization request to {}", url);

        let response = self
            .client
            .post(&url)
            .header("Authorization", self.auth_header())
            .multipart(form)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(self.handle_error_response(response).await);
        }

        let result: BlockingOptimizeResponse = response.json().await?;
        debug!("Optimization completed: {:?}", result);

        Ok(result)
    }

    /// Download a file from a URL
    pub async fn download_file(&self, url: &str) -> Result<Vec<u8>, ApiError> {
        debug!("Downloading file from {}", url);

        // The URL from the API is relative, make it absolute
        let full_url = if url.starts_with("http") {
            url.to_string()
        } else {
            // Use the base domain (not api subdomain) for downloads
            let base = self
                .config
                .api_url
                .replace("api.", "")
                .replace("://", "://www.");
            format!("{}{}", base.trim_end_matches('/'), url)
        };

        let response = self.client.get(&full_url).send().await?;

        if !response.status().is_success() {
            error!("Download failed with status: {}", response.status());
            return Err(ApiError::ServerError {
                message: format!("Download failed: {}", response.status()),
            });
        }

        let bytes = response.bytes().await?;
        Ok(bytes.to_vec())
    }
}
