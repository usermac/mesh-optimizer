//! File writing utilities for MCP server

use std::path::{Path, PathBuf};
use tokio::fs;
use tracing::debug;

/// Error type for file writing operations
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    #[error("Directory does not exist: {0}")]
    DirectoryNotFound(PathBuf),

    #[error("Cannot write to directory: {0}")]
    NotWritable(PathBuf),

    #[error("Failed to write file: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Invalid output path: {0}")]
    InvalidPath(String),
}

/// Generate an output path by adding "_optimized" suffix before the extension
///
/// Examples:
/// - "model.glb" -> "model_optimized.glb"
/// - "/path/to/mesh.obj" -> "/path/to/mesh_optimized.glb"
pub fn generate_output_path(input_path: &Path, output_extension: &str) -> PathBuf {
    let stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    let parent = input_path.parent().unwrap_or(Path::new("."));

    let new_filename = format!("{}_optimized.{}", stem, output_extension);
    parent.join(new_filename)
}

/// Generate output path in a specific directory
pub fn generate_output_path_in_dir(
    input_path: &Path,
    output_dir: &Path,
    output_extension: &str,
) -> PathBuf {
    let stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    let new_filename = format!("{}_optimized.{}", stem, output_extension);
    output_dir.join(new_filename)
}

/// Validate that the output directory exists and is writable
pub async fn validate_output_dir(path: &Path) -> Result<(), WriteError> {
    if !path.exists() {
        return Err(WriteError::DirectoryNotFound(path.to_path_buf()));
    }

    if !path.is_dir() {
        return Err(WriteError::NotWritable(path.to_path_buf()));
    }

    // Try to check if we can write to the directory by checking metadata
    let metadata = fs::metadata(path).await?;
    if metadata.permissions().readonly() {
        return Err(WriteError::NotWritable(path.to_path_buf()));
    }

    Ok(())
}

/// Write data to a file
pub async fn write_file(path: &Path, data: &[u8]) -> Result<(), WriteError> {
    debug!("Writing {} bytes to {:?}", data.len(), path);

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).await?;
        }
    }

    fs::write(path, data).await?;

    debug!("Successfully wrote file: {:?}", path);
    Ok(())
}

/// Save optimized files to disk
///
/// Returns a list of paths where files were saved
pub async fn save_optimized_files(
    input_path: &Path,
    output_dir: Option<&Path>,
    glb_data: Option<Vec<u8>>,
    usdz_data: Option<Vec<u8>>,
) -> Result<Vec<PathBuf>, WriteError> {
    let mut saved_paths = Vec::new();

    // Determine output directory
    let dir = output_dir.unwrap_or_else(|| input_path.parent().unwrap_or(Path::new(".")));

    // Validate output directory
    validate_output_dir(dir).await?;

    // Save GLB if provided
    if let Some(data) = glb_data {
        let path = generate_output_path_in_dir(input_path, dir, "glb");
        write_file(&path, &data).await?;
        saved_paths.push(path);
    }

    // Save USDZ if provided
    if let Some(data) = usdz_data {
        let path = generate_output_path_in_dir(input_path, dir, "usdz");
        write_file(&path, &data).await?;
        saved_paths.push(path);
    }

    Ok(saved_paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_output_path() {
        let input = Path::new("/path/to/model.glb");
        let output = generate_output_path(input, "glb");
        assert_eq!(output, PathBuf::from("/path/to/model_optimized.glb"));
    }

    #[test]
    fn test_generate_output_path_different_extension() {
        let input = Path::new("/path/to/model.obj");
        let output = generate_output_path(input, "glb");
        assert_eq!(output, PathBuf::from("/path/to/model_optimized.glb"));
    }

    #[test]
    fn test_generate_output_path_in_dir() {
        let input = Path::new("/source/model.glb");
        let output_dir = Path::new("/output");
        let output = generate_output_path_in_dir(input, output_dir, "usdz");
        assert_eq!(output, PathBuf::from("/output/model_optimized.usdz"));
    }
}
