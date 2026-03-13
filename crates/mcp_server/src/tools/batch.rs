//! Batch optimize tool implementation

use crate::api::{MeshOptClient, OptimizeParams};
use crate::files::{expand_path, read_file, save_optimized_files, ReadError, SUPPORTED_EXTENSIONS};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tracing::{debug, error, info, warn};

/// Input parameters for the optimize_batch tool
#[derive(Debug, Deserialize)]
pub struct OptimizeBatchInput {
    /// Directory path containing 3D files
    pub directory: String,

    /// Glob pattern to match files (default: "*.glb")
    #[serde(default)]
    pub pattern: Option<String>,

    /// Processing mode: "decimate" for polygon reduction, "remesh" for retopology
    pub mode: String,

    /// Target reduction ratio (0.0-1.0) for decimate mode
    #[serde(default)]
    pub ratio: Option<f32>,

    /// Target face count for remesh mode
    #[serde(default)]
    pub faces: Option<u32>,

    /// Texture size for remesh mode (256, 512, 1024, 2048, 4096, 8192)
    #[serde(default)]
    pub texture_size: Option<u32>,

    /// Output format: "glb", "usdz", or "both" (default: "glb")
    #[serde(default)]
    pub format: Option<String>,

    /// Optional output directory (default: same directory as input files)
    #[serde(default)]
    pub output_dir: Option<String>,
}

/// Result for a single file in the batch
#[derive(Debug, Serialize)]
pub struct FileResult {
    /// Input file path
    pub file: String,

    /// Whether this file was processed successfully
    pub success: bool,

    /// Output file path(s) if successful
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_paths: Option<Vec<String>>,

    /// Credits used for this file
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credits_used: Option<i32>,

    /// Error message if failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Output from the optimize_batch tool
#[derive(Debug, Serialize)]
pub struct OptimizeBatchOutput {
    /// Whether the overall operation succeeded (at least one file processed)
    pub success: bool,

    /// Total number of files found matching pattern
    pub total_files: usize,

    /// Number of files successfully processed
    pub successful: usize,

    /// Number of files that failed
    pub failed: usize,

    /// Total credits used across all files
    pub total_credits_used: i32,

    /// Remaining credit balance after processing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credits_remaining: Option<i32>,

    /// Per-file results
    pub results: Vec<FileResult>,

    /// Error message if the entire operation failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Error type for batch operations
#[derive(Debug, thiserror::Error)]
pub enum BatchError {
    #[error("Invalid mode: {0}. Must be 'decimate' or 'remesh'")]
    InvalidMode(String),

    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("Directory not found: {0}")]
    DirectoryNotFound(PathBuf),

    #[error("No files found matching pattern: {0}")]
    NoFilesFound(String),

    #[error("Path error: {0}")]
    PathError(#[from] ReadError),

    #[error("Glob pattern error: {0}")]
    GlobError(String),
}

impl From<BatchError> for OptimizeBatchOutput {
    fn from(err: BatchError) -> Self {
        OptimizeBatchOutput {
            success: false,
            total_files: 0,
            successful: 0,
            failed: 0,
            total_credits_used: 0,
            credits_remaining: None,
            results: vec![],
            error: Some(err.to_string()),
        }
    }
}

/// Validate the input parameters
fn validate_input(input: &OptimizeBatchInput) -> Result<(), BatchError> {
    // Validate mode
    match input.mode.as_str() {
        "decimate" => {
            if let Some(ratio) = input.ratio {
                if !(0.0..=1.0).contains(&ratio) {
                    return Err(BatchError::InvalidParameter(
                        "ratio must be between 0.0 and 1.0".to_string(),
                    ));
                }
            }
        }
        "remesh" => {
            if let Some(texture_size) = input.texture_size {
                let valid_sizes = [256, 512, 1024, 2048, 4096, 8192];
                if !valid_sizes.contains(&texture_size) {
                    return Err(BatchError::InvalidParameter(format!(
                        "texture_size must be one of: {:?}",
                        valid_sizes
                    )));
                }
            }
        }
        _ => {
            return Err(BatchError::InvalidMode(input.mode.clone()));
        }
    }

    // Validate format if provided
    if let Some(ref format) = input.format {
        let valid_formats = ["glb", "usdz", "both"];
        if !valid_formats.contains(&format.as_str()) {
            return Err(BatchError::InvalidParameter(format!(
                "format must be one of: {:?}",
                valid_formats
            )));
        }
    }

    Ok(())
}

/// Find files matching the pattern in the directory
fn find_matching_files(dir: &PathBuf, pattern: &str) -> Result<Vec<PathBuf>, BatchError> {
    let full_pattern = dir.join(pattern);
    let pattern_str = full_pattern.to_string_lossy();

    debug!("Searching for files matching: {}", pattern_str);

    let mut files = Vec::new();

    // Use simple pattern matching since we don't have the glob crate
    // Match files in the directory that match the pattern
    let entries = std::fs::read_dir(dir).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            BatchError::DirectoryNotFound(dir.clone())
        } else {
            BatchError::GlobError(e.to_string())
        }
    })?;

    // Parse pattern - support simple wildcards like "*.glb" or "*"
    let (prefix, suffix) = if let Some(star_pos) = pattern.find('*') {
        let prefix = &pattern[..star_pos];
        let suffix = &pattern[star_pos + 1..];
        (prefix, suffix)
    } else {
        // No wildcard - exact match
        (pattern, "")
    };

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                warn!("Failed to read directory entry: {}", e);
                continue;
            }
        };

        let path = entry.path();

        // Skip directories
        if path.is_dir() {
            continue;
        }

        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => continue,
        };

        // Check if file matches pattern
        let matches = if pattern == "*" {
            true
        } else if prefix.is_empty() && !suffix.is_empty() {
            // Pattern like "*.glb"
            file_name.ends_with(suffix)
        } else if !prefix.is_empty() && suffix.is_empty() {
            // Pattern like "model*"
            file_name.starts_with(prefix)
        } else if !prefix.is_empty() && !suffix.is_empty() {
            // Pattern like "model*.glb"
            file_name.starts_with(prefix) && file_name.ends_with(suffix)
        } else {
            // Exact match
            file_name == pattern
        };

        if matches {
            // Verify it has a supported extension
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default();

            if SUPPORTED_EXTENSIONS.contains(&ext.as_str()) {
                files.push(path);
            }
        }
    }

    // Sort files alphabetically for consistent ordering
    files.sort();

    debug!("Found {} matching files", files.len());
    Ok(files)
}

/// Execute the optimize_batch tool
pub async fn execute(client: &MeshOptClient, input: OptimizeBatchInput) -> OptimizeBatchOutput {
    match execute_inner(client, input).await {
        Ok(output) => output,
        Err(err) => err.into(),
    }
}

async fn execute_inner(
    client: &MeshOptClient,
    input: OptimizeBatchInput,
) -> Result<OptimizeBatchOutput, BatchError> {
    info!("Batch optimizing files in: {}", input.directory);

    // Validate input parameters
    validate_input(&input)?;

    // Expand and validate directory path
    let dir_path = expand_path(&input.directory)?;
    debug!("Expanded directory path: {:?}", dir_path);

    if !dir_path.exists() || !dir_path.is_dir() {
        return Err(BatchError::DirectoryNotFound(dir_path));
    }

    // Get pattern (default to *.glb)
    let pattern = input.pattern.as_deref().unwrap_or("*.glb");

    // Find matching files
    let files = find_matching_files(&dir_path, pattern)?;

    if files.is_empty() {
        return Err(BatchError::NoFilesFound(format!(
            "{}/{}",
            dir_path.display(),
            pattern
        )));
    }

    info!("Found {} files to process", files.len());

    // Build shared API parameters
    let params = OptimizeParams {
        mode: input.mode.clone(),
        ratio: input.ratio,
        faces: input.faces,
        texture_size: input.texture_size,
        format: input.format.clone(),
    };

    // Determine output directory
    let output_dir = match &input.output_dir {
        Some(dir) => Some(expand_path(dir)?),
        None => None,
    };

    // Process files one-by-one
    let mut results = Vec::new();
    let mut successful = 0;
    let mut failed = 0;
    let mut total_credits_used = 0i32;
    let mut credits_remaining = None;

    for file_path in &files {
        let file_name = file_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        info!("Processing: {}", file_name);

        // Process this file
        let result = process_single_file(client, file_path, &params, output_dir.as_ref(), &input.format).await;

        match result {
            Ok((file_result, used, remaining)) => {
                if let Some(used) = used {
                    total_credits_used += used;
                }
                credits_remaining = remaining;
                successful += 1;
                results.push(file_result);
            }
            Err(err) => {
                error!("Failed to process {}: {}", file_name, err);
                failed += 1;
                results.push(FileResult {
                    file: file_path.display().to_string(),
                    success: false,
                    output_paths: None,
                    credits_used: None,
                    error: Some(err.to_string()),
                });
            }
        }
    }

    let total_files = files.len();
    let success = successful > 0;

    info!(
        "Batch complete: {}/{} successful, {} credits used",
        successful, total_files, total_credits_used
    );

    Ok(OptimizeBatchOutput {
        success,
        total_files,
        successful,
        failed,
        total_credits_used,
        credits_remaining,
        results,
        error: None,
    })
}

/// Process a single file and return the result
async fn process_single_file(
    client: &MeshOptClient,
    file_path: &PathBuf,
    params: &OptimizeParams,
    output_dir: Option<&PathBuf>,
    format: &Option<String>,
) -> Result<(FileResult, Option<i32>, Option<i32>), String> {
    // Read the input file
    let file_data = read_file(file_path)
        .await
        .map_err(|e| format!("Failed to read file: {}", e))?;

    debug!("Read {} bytes from {:?}", file_data.len(), file_path);

    // Call the API
    let response = client
        .optimize(file_path, file_data, params)
        .await
        .map_err(|e| format!("API error: {}", e))?;

    // Extract completed status
    let completed = match response.status {
        crate::api::types::JobStatusInner::Completed { Completed: status } => status,
        crate::api::types::JobStatusInner::Failed { Failed: failed } => {
            return Err(format!("Optimization failed: {}", failed.error));
        }
        crate::api::types::JobStatusInner::Processing(status) => {
            return Err(format!("Unexpected status: {}", status));
        }
    };

    // Download and save files based on format
    let format_str = format.as_deref().unwrap_or("glb");
    let mut glb_data = None;
    let mut usdz_data = None;

    if format_str == "glb" || format_str == "both" {
        if !completed.glb_url.is_empty() {
            glb_data = Some(
                client
                    .download_file(&completed.glb_url)
                    .await
                    .map_err(|e| format!("Failed to download GLB: {}", e))?,
            );
        }
    }

    if format_str == "usdz" || format_str == "both" {
        if !completed.usdz_url.is_empty() {
            usdz_data = Some(
                client
                    .download_file(&completed.usdz_url)
                    .await
                    .map_err(|e| format!("Failed to download USDZ: {}", e))?,
            );
        }
    }

    // Save files to disk
    let saved_paths = save_optimized_files(file_path, output_dir.map(|p| p.as_path()), glb_data, usdz_data)
        .await
        .map_err(|e| format!("Failed to save files: {}", e))?;

    Ok((
        FileResult {
            file: file_path.display().to_string(),
            success: true,
            output_paths: Some(saved_paths.iter().map(|p| p.display().to_string()).collect()),
            credits_used: completed.credits_used,
            error: None,
        },
        completed.credits_used,
        completed.credits_remaining,
    ))
}

/// Get the tool definition for MCP
pub fn tool_definition() -> serde_json::Value {
    serde_json::json!({
        "name": "optimize_batch",
        "description": "Optimize multiple 3D mesh files in a directory. Processes all matching files sequentially and downloads each result immediately. Returns aggregated results with per-file status.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "directory": {
                    "type": "string",
                    "description": "Directory path containing 3D files. Supports ~ for home directory."
                },
                "pattern": {
                    "type": "string",
                    "description": "Glob pattern to match files. Examples: '*.glb', '*.fbx', 'model*.obj'. Default: '*.glb'",
                    "default": "*.glb"
                },
                "mode": {
                    "type": "string",
                    "enum": ["decimate", "remesh"],
                    "description": "Processing mode: 'decimate' for fast polygon reduction (1 credit each), 'remesh' for high-quality retopology with texture baking (2 credits each)"
                },
                "ratio": {
                    "type": "number",
                    "minimum": 0,
                    "maximum": 1,
                    "description": "Target reduction ratio for decimate mode (0.0-1.0). Example: 0.5 keeps 50% of polygons. Default: 0.5"
                },
                "faces": {
                    "type": "integer",
                    "minimum": 100,
                    "description": "Target face count for remesh mode. Default: 10000"
                },
                "texture_size": {
                    "type": "integer",
                    "enum": [256, 512, 1024, 2048, 4096, 8192],
                    "description": "Texture resolution for remesh mode. Default: 1024"
                },
                "format": {
                    "type": "string",
                    "enum": ["glb", "usdz", "both"],
                    "description": "Output format. Default: 'glb'"
                },
                "output_dir": {
                    "type": "string",
                    "description": "Output directory for optimized files. Default: same directory as input files"
                }
            },
            "required": ["directory", "mode"]
        }
    })
}
