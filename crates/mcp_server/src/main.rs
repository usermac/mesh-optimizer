//! MeshOpt MCP Server entry point
//!
//! Runs the MCP server over stdio, handling JSON-RPC requests from MCP clients.

use meshopt_mcp::{Config, McpHandler, MeshOptClient};
use std::io::{self, BufRead, Write};
use tracing::{debug, error, info, Level};
use tracing_subscriber::FmtSubscriber;

fn main() {
    // Load configuration
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Configuration error: {}", e);
            eprintln!("Set MESHOPT_API_KEY environment variable with your API key.");
            eprintln!("Get your API key at: https://webdeliveryengine.com");
            std::process::exit(1);
        }
    };

    // Initialize logging
    let log_level = if config.debug {
        Level::DEBUG
    } else {
        Level::WARN
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .with_writer(io::stderr)
        .with_ansi(false)
        .compact()
        .finish();

    tracing::subscriber::set_global_default(subscriber).expect("Failed to set tracing subscriber");

    info!("Starting MeshOpt MCP server v{}", env!("CARGO_PKG_VERSION"));
    debug!("API URL: {}", config.api_url);

    // Create API client
    let client = MeshOptClient::new(config);

    // Create MCP handler
    let mut handler = McpHandler::new(client);

    // Create async runtime for handling requests
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");

    // Read from stdin, write to stdout
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(e) => {
                error!("Failed to read from stdin: {}", e);
                break;
            }
        };

        // Skip empty lines
        if line.trim().is_empty() {
            continue;
        }

        debug!("Received: {}", line);

        // Parse the request
        let request = match McpHandler::parse_request(&line) {
            Ok(req) => req,
            Err(response) => {
                // Send error response
                let json = serde_json::to_string(&response).unwrap_or_default();
                if writeln!(stdout, "{}", json).is_err() {
                    break;
                }
                let _ = stdout.flush();
                continue;
            }
        };

        // Handle the request
        let response = runtime.block_on(handler.handle_request(request));

        // Send response if there is one (notifications don't get responses)
        if let Some(response) = response {
            let json = match serde_json::to_string(&response) {
                Ok(json) => json,
                Err(e) => {
                    error!("Failed to serialize response: {}", e);
                    continue;
                }
            };

            debug!("Sending: {}", json);

            if writeln!(stdout, "{}", json).is_err() {
                error!("Failed to write to stdout");
                break;
            }

            if stdout.flush().is_err() {
                error!("Failed to flush stdout");
                break;
            }
        }
    }

    info!("MeshOpt MCP server shutting down");
}
