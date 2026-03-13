//! File reading utilities for MCP server

use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::debug;

/// Supported input file extensions
pub const SUPPORTED_EXTENSIONS: &[&str] = &["glb", "gltf", "obj", "fbx", "zip", "usdz"];

/// Error type for file reading operations
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("File not found: {0}")]
    NotFound(PathBuf),

    #[error("Unsupported file type: {0}. Supported: {}", SUPPORTED_EXTENSIONS.join(", "))]
    UnsupportedType(String),

    #[error("File is empty: {0}")]
    EmptyFile(PathBuf),

    #[error("Failed to read file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Invalid path: {0}")]
    InvalidPath(String),
}

/// Expand a path, handling ~ for home directory
pub fn expand_path(path: &str) -> Result<PathBuf, ReadError> {
    let path = path.trim();

    if path.starts_with("~/") {
        let home = dirs::home_dir().ok_or_else(|| {
            ReadError::InvalidPath("Could not determine home directory".to_string())
        })?;
        Ok(home.join(&path[2..]))
    } else if path.starts_with('~') && path.len() == 1 {
        dirs::home_dir()
            .ok_or_else(|| ReadError::InvalidPath("Could not determine home directory".to_string()))
    } else {
        Ok(PathBuf::from(path))
    }
}

/// Validate that a file exists and has a supported extension
pub fn validate_file_path(path: &Path) -> Result<(), ReadError> {
    // Check extension
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    if !SUPPORTED_EXTENSIONS.contains(&ext.as_str()) {
        return Err(ReadError::UnsupportedType(ext));
    }

    Ok(())
}

/// Read a file from the local filesystem
pub async fn read_file(path: &Path) -> Result<Vec<u8>, ReadError> {
    debug!("Reading file: {:?}", path);

    // Check if file exists
    if !path.exists() {
        return Err(ReadError::NotFound(path.to_path_buf()));
    }

    // Validate extension
    validate_file_path(path)?;

    // Read file contents
    let data = fs::read(path).await?;

    // Check if file is empty
    if data.is_empty() {
        return Err(ReadError::EmptyFile(path.to_path_buf()));
    }

    debug!("Read {} bytes from {:?}", data.len(), path);
    Ok(data)
}

/// Get the file size without reading the entire file
pub async fn get_file_size(path: &Path) -> Result<u64, ReadError> {
    let metadata = fs::metadata(path).await?;
    Ok(metadata.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_path_home() {
        let expanded = expand_path("~/test.glb").unwrap();
        assert!(expanded.to_string_lossy().contains("test.glb"));
        assert!(!expanded.to_string_lossy().starts_with("~"));
    }

    #[test]
    fn test_expand_path_absolute() {
        let expanded = expand_path("/tmp/test.glb").unwrap();
        assert_eq!(expanded, PathBuf::from("/tmp/test.glb"));
    }

    #[test]
    fn test_validate_supported_extension() {
        assert!(validate_file_path(Path::new("model.glb")).is_ok());
        assert!(validate_file_path(Path::new("model.gltf")).is_ok());
        assert!(validate_file_path(Path::new("model.obj")).is_ok());
        assert!(validate_file_path(Path::new("model.fbx")).is_ok());
        assert!(validate_file_path(Path::new("model.zip")).is_ok());
        assert!(validate_file_path(Path::new("model.usdz")).is_ok());
    }

    #[test]
    fn test_validate_unsupported_extension() {
        assert!(validate_file_path(Path::new("model.txt")).is_err());
        assert!(validate_file_path(Path::new("model.png")).is_err());
    }
}
