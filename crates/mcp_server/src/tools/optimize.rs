//! Optimize mesh tool implementation

use crate::api::{ApiError, MeshOptClient, OptimizeParams};
use crate::files::{expand_path, read_file, save_optimized_files, ReadError, WriteError};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

/// Input parameters for the optimize_mesh tool
#[derive(Debug, Deserialize)]
pub struct OptimizeMeshInput {
    /// Path to the 3D model file (GLB, GLTF, OBJ, FBX, or ZIP)
    pub file_path: String,

    /// Processing mode: "decimate" for polygon reduction, "remesh" for retopology
    pub mode: String,

    /// Target reduction ratio (0.0-1.0) for decimate mode
    /// Example: 0.5 keeps 50% of polygons
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

    /// Optional output directory (default: same directory as input)
    #[serde(default)]
    pub output_dir: Option<String>,
}

/// Output from the optimize_mesh tool
#[derive(Debug, Serialize)]
pub struct OptimizeMeshOutput {
    /// Whether the operation succeeded
    pub success: bool,

    /// Local paths where optimized files were saved
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_paths: Option<Vec<String>>,

    /// Original face count
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_faces: Option<u64>,

    /// Output face count
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_faces: Option<u64>,

    /// Reduction percentage
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reduction_percent: Option<f64>,

    /// Credits used for this operation
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credits_used: Option<i32>,

    /// Remaining credit balance
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credits_remaining: Option<i32>,

    /// Error message if operation failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Error type for optimize operations
#[derive(Debug, thiserror::Error)]
pub enum OptimizeError {
    #[error("Invalid mode: {0}. Must be 'decimate' or 'remesh'")]
    InvalidMode(String),

    #[error("Missing required parameter: {0}")]
    MissingParameter(String),

    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),

    #[error("File error: {0}")]
    FileRead(#[from] ReadError),

    #[error("Write error: {0}")]
    FileWrite(#[from] WriteError),

    #[error("API error: {0}")]
    Api(#[from] ApiError),
}

impl From<OptimizeError> for OptimizeMeshOutput {
    fn from(err: OptimizeError) -> Self {
        OptimizeMeshOutput {
            success: false,
            output_paths: None,
            original_faces: None,
            output_faces: None,
            reduction_percent: None,
            credits_used: None,
            credits_remaining: None,
            error: Some(err.to_string()),
        }
    }
}

/// Validate the input parameters
fn validate_input(input: &OptimizeMeshInput) -> Result<(), OptimizeError> {
    // Validate mode
    match input.mode.as_str() {
        "decimate" => {
            // Ratio is optional for decimate (API has default)
            if let Some(ratio) = input.ratio {
                if !(0.0..=1.0).contains(&ratio) {
                    return Err(OptimizeError::InvalidParameter(
                        "ratio must be between 0.0 and 1.0".to_string(),
                    ));
                }
            }
        }
        "remesh" => {
            // Faces is optional for remesh (API has default)
            if let Some(texture_size) = input.texture_size {
                let valid_sizes = [256, 512, 1024, 2048, 4096, 8192];
                if !valid_sizes.contains(&texture_size) {
                    return Err(OptimizeError::InvalidParameter(format!(
                        "texture_size must be one of: {:?}",
                        valid_sizes
                    )));
                }
            }
        }
        _ => {
            return Err(OptimizeError::InvalidMode(input.mode.clone()));
        }
    }

    // Validate format if provided
    if let Some(ref format) = input.format {
        let valid_formats = ["glb", "usdz", "both"];
        if !valid_formats.contains(&format.as_str()) {
            return Err(OptimizeError::InvalidParameter(format!(
                "format must be one of: {:?}",
                valid_formats
            )));
        }
    }

    Ok(())
}

/// Execute the optimize_mesh tool
pub async fn execute(client: &MeshOptClient, input: OptimizeMeshInput) -> OptimizeMeshOutput {
    match execute_inner(client, input).await {
        Ok(output) => output,
        Err(err) => err.into(),
    }
}

async fn execute_inner(
    client: &MeshOptClient,
    input: OptimizeMeshInput,
) -> Result<OptimizeMeshOutput, OptimizeError> {
    info!("Optimizing mesh: {}", input.file_path);

    // Validate input parameters
    validate_input(&input)?;

    // Expand and validate file path
    let file_path = expand_path(&input.file_path)?;
    debug!("Expanded path: {:?}", file_path);

    // Read the input file
    let file_data = read_file(&file_path).await?;
    debug!("Read {} bytes from input file", file_data.len());

    // Build API parameters
    let params = OptimizeParams {
        mode: input.mode.clone(),
        ratio: input.ratio,
        faces: input.faces,
        texture_size: input.texture_size,
        format: input.format.clone(),
    };

    // Call the API
    let response = client.optimize(&file_path, file_data, &params).await?;
    debug!("API response: {:?}", response);

    // Extract completed status
    let completed = match response.status {
        crate::api::types::JobStatusInner::Completed { Completed: status } => status,
        crate::api::types::JobStatusInner::Failed { Failed: failed } => {
            return Err(OptimizeError::Api(ApiError::ServerError {
                message: failed.error,
            }));
        }
        crate::api::types::JobStatusInner::Processing(status) => {
            return Err(OptimizeError::Api(ApiError::ServerError {
                message: format!("Unexpected status: {}", status),
            }));
        }
    };

    // Determine output directory
    let output_dir = match &input.output_dir {
        Some(dir) => Some(expand_path(dir)?),
        None => None,
    };

    // Download and save files based on format
    let format = input.format.as_deref().unwrap_or("glb");
    let mut glb_data = None;
    let mut usdz_data = None;

    if format == "glb" || format == "both" {
        if !completed.glb_url.is_empty() {
            glb_data = Some(client.download_file(&completed.glb_url).await?);
        }
    }

    if format == "usdz" || format == "both" {
        if !completed.usdz_url.is_empty() {
            usdz_data = Some(client.download_file(&completed.usdz_url).await?);
        }
    }

    // Save files to disk
    let saved_paths = save_optimized_files(
        &file_path,
        output_dir.as_deref(),
        glb_data,
        usdz_data,
    )
    .await?;

    // Calculate reduction percentage
    let reduction_percent = match (completed.original_faces, completed.output_faces) {
        (Some(orig), Some(out)) if orig > 0 => {
            Some(((orig as f64 - out as f64) / orig as f64) * 100.0)
        }
        _ => None,
    };

    info!(
        "Optimization complete. Saved {} file(s)",
        saved_paths.len()
    );

    Ok(OptimizeMeshOutput {
        success: true,
        output_paths: Some(saved_paths.iter().map(|p| p.display().to_string()).collect()),
        original_faces: completed.original_faces,
        output_faces: completed.output_faces,
        reduction_percent,
        credits_used: completed.credits_used,
        credits_remaining: completed.credits_remaining,
        error: None,
    })
}

/// Get the tool definition for MCP
pub fn tool_definition() -> serde_json::Value {
    serde_json::json!({
        "name": "optimize_mesh",
        "description": "Optimize a 3D mesh file for web/mobile delivery. Supports GLB, GLTF, OBJ, FBX, and ZIP files. Two modes available: 'decimate' for fast polygon reduction, 'remesh' for high-quality retopology with texture baking.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Path to the 3D model file. Supports ~ for home directory."
                },
                "mode": {
                    "type": "string",
                    "enum": ["decimate", "remesh"],
                    "description": "Processing mode: 'decimate' for fast polygon reduction (1 credit), 'remesh' for high-quality retopology with texture baking (2 credits)"
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
                    "description": "Output directory for optimized files. Default: same directory as input file"
                }
            },
            "required": ["file_path", "mode"]
        }
    })
}
