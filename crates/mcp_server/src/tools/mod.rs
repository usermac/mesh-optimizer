//! MCP tool implementations
//!
//! This module contains the tools exposed by the MeshOpt MCP server:
//! - `optimize_mesh`: Optimize a single 3D mesh file
//! - `optimize_batch`: Optimize multiple 3D mesh files in a directory
//! - `check_balance`: Check credit balance
//! - `get_usage`: Get usage history

pub mod balance;
pub mod batch;
pub mod optimize;
pub mod usage;

use serde_json::Value;

/// Get all tool definitions for MCP
pub fn get_tool_definitions() -> Vec<Value> {
    vec![
        optimize::tool_definition(),
        batch::tool_definition(),
        balance::tool_definition(),
        usage::tool_definition(),
    ]
}
