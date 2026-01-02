//! MeshOpt API client module
//!
//! Provides HTTP client functionality for communicating with the MeshOpt API.

pub mod client;
pub mod error;
pub mod types;

pub use client::MeshOptClient;
pub use error::ApiError;
pub use types::*;
