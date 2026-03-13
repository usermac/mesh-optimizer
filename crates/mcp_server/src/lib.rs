//! MeshOpt MCP Server
//!
//! A Model Context Protocol (MCP) server for the MeshOpt 3D mesh optimization API.
//!
//! This server exposes three tools:
//! - `optimize_mesh`: Optimize a 3D mesh file for web/mobile delivery
//! - `check_balance`: Check your credit balance
//! - `get_usage`: View your usage history
//!
//! ## Configuration
//!
//! Set the following environment variables:
//! - `MESHOPT_API_KEY`: Your MeshOpt API key (required)
//! - `MESHOPT_API_URL`: API base URL (optional, defaults to https://api.webdeliveryengine.com)
//! - `MESHOPT_DEBUG`: Enable debug logging (optional, set to "1" or "true")

pub mod api;
pub mod config;
pub mod files;
pub mod server;
pub mod tools;

pub use api::MeshOptClient;
pub use config::Config;
pub use server::McpHandler;
