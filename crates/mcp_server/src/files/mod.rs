//! File handling utilities for MCP server
//!
//! Provides functionality for reading input files and writing optimized outputs.

pub mod reader;
pub mod writer;

pub use reader::{expand_path, read_file, validate_file_path, ReadError, SUPPORTED_EXTENSIONS};
pub use writer::{
    generate_output_path, generate_output_path_in_dir, save_optimized_files, write_file,
    WriteError,
};
