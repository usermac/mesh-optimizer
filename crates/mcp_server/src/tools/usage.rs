//! Get usage history tool implementation

use crate::api::{ApiError, HistoryEntry, MeshOptClient};
use serde::{Deserialize, Serialize};
use tracing::info;

/// Input parameters for the get_usage tool
#[derive(Debug, Deserialize, Default)]
pub struct GetUsageInput {
    /// Maximum number of history entries to return (default: 10)
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Output from the get_usage tool
#[derive(Debug, Serialize)]
pub struct GetUsageOutput {
    /// Whether the operation succeeded
    pub success: bool,

    /// Usage history entries
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<Vec<UsageEntry>>,

    /// Error message if operation failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// A formatted usage history entry
#[derive(Debug, Serialize)]
pub struct UsageEntry {
    /// Entry ID
    pub id: i64,

    /// Timestamp of the entry
    pub timestamp: String,

    /// Type of entry: "optimization" or "purchase"
    #[serde(rename = "type")]
    pub entry_type: String,

    /// Credits used (negative) or purchased (positive)
    pub credits: i32,

    /// Filename for optimization entries
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,

    /// Mode for optimization entries (decimate/remesh)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,

    /// Parameters used (e.g., "50%" for ratio)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameters: Option<String>,

    /// Whether this was a free re-optimization
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_free_reopt: Option<bool>,
}

impl From<HistoryEntry> for UsageEntry {
    fn from(entry: HistoryEntry) -> Self {
        UsageEntry {
            id: entry.id,
            timestamp: entry.timestamp,
            entry_type: entry.entry_type,
            credits: entry.credits,
            filename: entry.filename,
            mode: entry.mode,
            parameters: entry.parameters,
            is_free_reopt: entry.is_free_reopt,
        }
    }
}

/// Execute the get_usage tool
pub async fn execute(client: &MeshOptClient, input: GetUsageInput) -> GetUsageOutput {
    let limit = input.limit.unwrap_or(10);
    info!("Fetching usage history (limit: {})", limit);

    match client.get_history(Some(limit)).await {
        Ok(entries) => {
            info!("Retrieved {} history entries", entries.len());
            GetUsageOutput {
                success: true,
                history: Some(entries.into_iter().map(UsageEntry::from).collect()),
                error: None,
            }
        }
        Err(err) => {
            let message = match &err {
                ApiError::Unauthorized { .. } => err.user_message(),
                _ => err.to_string(),
            };

            GetUsageOutput {
                success: false,
                history: None,
                error: Some(message),
            }
        }
    }
}

/// Get the tool definition for MCP
pub fn tool_definition() -> serde_json::Value {
    serde_json::json!({
        "name": "get_usage",
        "description": "Get your MeshOpt usage history showing recent optimizations and credit purchases. Each entry shows the operation type, credits used, and for optimizations: the filename, mode, and parameters used.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "limit": {
                    "type": "integer",
                    "minimum": 1,
                    "maximum": 100,
                    "description": "Maximum number of history entries to return. Default: 10"
                }
            },
            "required": []
        }
    })
}
