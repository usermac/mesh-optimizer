//! MCP server implementation
//!
//! Provides the JSON-RPC handler for the Model Context Protocol.

pub mod handler;

pub use handler::{JsonRpcRequest, JsonRpcResponse, McpHandler};
