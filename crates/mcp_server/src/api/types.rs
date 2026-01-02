//! API request and response types for MeshOpt

use serde::{Deserialize, Serialize};

/// Parameters for mesh optimization
#[derive(Debug, Clone, Default)]
pub struct OptimizeParams {
    /// Processing mode: "decimate" or "remesh"
    pub mode: String,

    /// Target reduction ratio (0.0-1.0) for decimate mode
    pub ratio: Option<f32>,

    /// Target face count for remesh mode
    pub faces: Option<u32>,

    /// Texture size for remesh mode (256, 512, 1024, 2048, 4096, 8192)
    pub texture_size: Option<u32>,

    /// Output format: "glb", "usdz", or "both"
    pub format: Option<String>,
}

/// Initial response from POST /optimize (non-blocking)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizeStartResponse {
    pub job_id: String,
    pub status: String,
    pub credits_used: i32,
    pub credits_remaining: i32,
}

/// Completed job status from the API
#[derive(Debug, Deserialize)]
pub struct CompletedStatus {
    pub output_size: u64,
    pub glb_url: String,
    pub usdz_url: String,
    pub expires_at: String,
    #[serde(default)]
    pub original_faces: Option<u64>,
    #[serde(default)]
    pub output_faces: Option<u64>,
    #[serde(default)]
    pub remesh_method: Option<String>,
    #[serde(default)]
    pub credits_used: Option<i32>,
    #[serde(default)]
    pub credits_remaining: Option<i32>,
}

/// Failed job status
#[derive(Debug, Deserialize)]
pub struct FailedStatus {
    pub error: String,
}

/// Job status enum matching API response
#[derive(Debug, Deserialize)]
#[serde(untagged)]
#[allow(non_snake_case)]
pub enum JobStatusInner {
    Completed { Completed: CompletedStatus },
    Failed { Failed: FailedStatus },
    Processing(String),
}

/// Full job status response from GET /job/:id
#[derive(Debug, Deserialize)]
pub struct JobStatusResponse {
    pub status: JobStatusInner,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub download_commands: Option<serde_json::Value>,
}

/// Blocking optimize response (same structure as job status)
pub type BlockingOptimizeResponse = JobStatusResponse;

/// Response from GET /credits
#[derive(Debug, Deserialize)]
pub struct CreditsResponse {
    pub credits: i32,
}

/// History entry from GET /history
#[derive(Debug, Deserialize, Serialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub timestamp: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    pub credits: i32,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub parameters: Option<String>,
    #[serde(default)]
    pub is_free_reopt: Option<bool>,
    #[serde(default)]
    pub raw_description: Option<String>,
}

/// Response from GET /history
#[derive(Debug, Deserialize)]
pub struct HistoryResponse {
    pub history: Vec<HistoryEntry>,
}

/// Result of a successful optimization
#[derive(Debug, Clone, Serialize)]
pub struct OptimizeResult {
    /// Local paths where optimized files were saved
    pub output_paths: Vec<String>,

    /// Original face count (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_faces: Option<u64>,

    /// Output face count (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_faces: Option<u64>,

    /// Reduction percentage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reduction_percent: Option<f64>,

    /// Credits used for this operation
    pub credits_used: i32,

    /// Remaining credit balance
    pub credits_remaining: i32,
}
