//! MCP JSON-RPC message handler
//!
//! Implements the Model Context Protocol (MCP) over stdio using JSON-RPC 2.0.

use crate::api::MeshOptClient;
use crate::tools::{self, balance, optimize, usage};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tracing::{debug, error, info, warn};

/// JSON-RPC request structure
#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

/// JSON-RPC response structure
#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC error structure
#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

// Standard JSON-RPC error codes
pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;

impl JsonRpcResponse {
    pub fn success(id: Option<Value>, result: Value) -> Self {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Option<Value>, code: i32, message: impl Into<String>) -> Self {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

/// MCP server handler
pub struct McpHandler {
    client: MeshOptClient,
    initialized: bool,
}

impl McpHandler {
    pub fn new(client: MeshOptClient) -> Self {
        McpHandler {
            client,
            initialized: false,
        }
    }

    /// Handle an incoming JSON-RPC request
    pub async fn handle_request(&mut self, request: JsonRpcRequest) -> Option<JsonRpcResponse> {
        debug!("Handling request: {} (id: {:?})", request.method, request.id);

        match request.method.as_str() {
            // MCP lifecycle methods
            "initialize" => Some(self.handle_initialize(request.id, request.params)),
            "initialized" => {
                // This is a notification, no response needed
                self.initialized = true;
                info!("MCP session initialized");
                None
            }
            "shutdown" => Some(self.handle_shutdown(request.id)),

            // MCP tool methods
            "tools/list" => Some(self.handle_tools_list(request.id)),
            "tools/call" => Some(self.handle_tools_call(request.id, request.params).await),

            // Unknown method
            _ => {
                warn!("Unknown method: {}", request.method);
                Some(JsonRpcResponse::error(
                    request.id,
                    METHOD_NOT_FOUND,
                    format!("Method not found: {}", request.method),
                ))
            }
        }
    }

    /// Handle initialize request
    fn handle_initialize(&mut self, id: Option<Value>, params: Option<Value>) -> JsonRpcResponse {
        info!("Initializing MCP server");

        // Log client info if provided
        if let Some(params) = params {
            if let Some(client_info) = params.get("clientInfo") {
                debug!("Client info: {:?}", client_info);
            }
        }

        JsonRpcResponse::success(
            id,
            json!({
                "protocolVersion": "2024-11-05",
                "serverInfo": {
                    "name": "meshopt-mcp",
                    "version": env!("CARGO_PKG_VERSION")
                },
                "capabilities": {
                    "tools": {}
                }
            }),
        )
    }

    /// Handle shutdown request
    fn handle_shutdown(&self, id: Option<Value>) -> JsonRpcResponse {
        info!("Shutting down MCP server");
        JsonRpcResponse::success(id, json!(null))
    }

    /// Handle tools/list request
    fn handle_tools_list(&self, id: Option<Value>) -> JsonRpcResponse {
        debug!("Listing available tools");
        let tool_defs = tools::get_tool_definitions();

        JsonRpcResponse::success(
            id,
            json!({
                "tools": tool_defs
            }),
        )
    }

    /// Handle tools/call request
    async fn handle_tools_call(
        &self,
        id: Option<Value>,
        params: Option<Value>,
    ) -> JsonRpcResponse {
        let params = match params {
            Some(p) => p,
            None => {
                return JsonRpcResponse::error(id, INVALID_PARAMS, "Missing params");
            }
        };

        let tool_name = match params.get("name").and_then(|n| n.as_str()) {
            Some(name) => name,
            None => {
                return JsonRpcResponse::error(id, INVALID_PARAMS, "Missing tool name");
            }
        };

        let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

        debug!("Calling tool: {} with args: {:?}", tool_name, arguments);

        let result = match tool_name {
            "optimize_mesh" => {
                let input: optimize::OptimizeMeshInput = match serde_json::from_value(arguments) {
                    Ok(input) => input,
                    Err(e) => {
                        return JsonRpcResponse::error(
                            id,
                            INVALID_PARAMS,
                            format!("Invalid arguments: {}", e),
                        );
                    }
                };
                let output = optimize::execute(&self.client, input).await;
                serde_json::to_value(output).unwrap_or(json!({"error": "Serialization failed"}))
            }

            "check_balance" => {
                let output = balance::execute(&self.client).await;
                serde_json::to_value(output).unwrap_or(json!({"error": "Serialization failed"}))
            }

            "get_usage" => {
                let input: usage::GetUsageInput = match serde_json::from_value(arguments) {
                    Ok(input) => input,
                    Err(e) => {
                        return JsonRpcResponse::error(
                            id,
                            INVALID_PARAMS,
                            format!("Invalid arguments: {}", e),
                        );
                    }
                };
                let output = usage::execute(&self.client, input).await;
                serde_json::to_value(output).unwrap_or(json!({"error": "Serialization failed"}))
            }

            _ => {
                return JsonRpcResponse::error(
                    id,
                    METHOD_NOT_FOUND,
                    format!("Unknown tool: {}", tool_name),
                );
            }
        };

        // Format result as MCP tool result
        let content = json!([{
            "type": "text",
            "text": serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string())
        }]);

        JsonRpcResponse::success(
            id,
            json!({
                "content": content,
                "isError": result.get("success").and_then(|v| v.as_bool()) == Some(false)
            }),
        )
    }

    /// Parse a JSON-RPC request from a string
    pub fn parse_request(line: &str) -> Result<JsonRpcRequest, JsonRpcResponse> {
        serde_json::from_str(line).map_err(|e| {
            error!("Failed to parse request: {}", e);
            JsonRpcResponse::error(None, PARSE_ERROR, format!("Parse error: {}", e))
        })
    }
}
