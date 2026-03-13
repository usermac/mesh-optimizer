//! Check balance tool implementation

use crate::api::{ApiError, MeshOptClient};
use serde::Serialize;
use tracing::info;

/// Output from the check_balance tool
#[derive(Debug, Serialize)]
pub struct CheckBalanceOutput {
    /// Whether the operation succeeded
    pub success: bool,

    /// Current credit balance
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credits: Option<i32>,

    /// Error message if operation failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Execute the check_balance tool
pub async fn execute(client: &MeshOptClient) -> CheckBalanceOutput {
    info!("Checking credit balance");

    match client.get_credits().await {
        Ok(credits) => {
            info!("Credit balance: {}", credits);
            CheckBalanceOutput {
                success: true,
                credits: Some(credits),
                error: None,
            }
        }
        Err(err) => {
            let message = match &err {
                ApiError::Unauthorized { .. } => err.user_message(),
                _ => err.to_string(),
            };

            CheckBalanceOutput {
                success: false,
                credits: None,
                error: Some(message),
            }
        }
    }
}

/// Get the tool definition for MCP
pub fn tool_definition() -> serde_json::Value {
    serde_json::json!({
        "name": "check_balance",
        "description": "Check your current MeshOpt credit balance. Credits are used for mesh optimization: decimate mode costs 1 credit, remesh mode costs 2 credits. Re-optimizing the same file within 24 hours is free.",
        "inputSchema": {
            "type": "object",
            "properties": {},
            "required": []
        }
    })
}
