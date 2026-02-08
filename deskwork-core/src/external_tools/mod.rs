//! External tool management for on-demand downloads.
//!
//! This module provides infrastructure for downloading and managing external tools
//! like UV. Tools are downloaded on-demand and stored in the OS temp directory
//! to keep the main application binary small.
//!
//! # Architecture
//!
//! - `types`: Core types (ExternalToolId, Platform, ToolStatus, ToolDefinition)
//! - `paths`: Temp-folder path resolution
//! - `catalog`: Static tool definitions with download URLs
//! - `manifest`: JSON persistence for installed tool state
//! - `downloader`: Async file download with progress reporting
//! - `extractor`: Archive extraction (zip, tar.gz)
//! - `env`: Environment variable management for subprocesses
//! - `manager`: High-level API for managing tools
//!
//! # Example
//!
//! ```ignore
//! use deskwork_core::external_tools::{ExternalToolManager, ExternalToolId};
//!
//! let manager = ExternalToolManager::new()?;
//!
//! // Install UV with progress reporting
//! manager.install(ExternalToolId::Uv, |progress| {
//!     if let Some(percent) = progress.percent {
//!         println!("Progress: {:.1}%", percent);
//!     }
//! }).await?;
//!
//! // Get the executable path
//! if let Some(uv_path) = manager.get_executable_path(ExternalToolId::Uv).await {
//!     println!("UV installed at: {}", uv_path.display());
//! }
//! ```

pub mod catalog;
pub mod downloader;
pub mod env;
pub mod extractor;
pub mod manager;
pub mod manifest;
pub mod paths;
pub mod types;

// Re-export commonly used types
pub use catalog::{get_all_tool_definitions, get_tool_definition};
pub use downloader::DownloadProgress;
pub use env::{apply_to_command, env_overrides, installed_tool_bin_dirs, prepend_tools_to_path};
pub use manager::{is_fuse_available, ExternalToolManager, ToolInfo, FUSE_DOCS_URL};
pub use manifest::{load_manifest, save_manifest, InstalledToolInfo, ToolsManifest};
pub use paths::{
    ensure_dirs_exist, get_deskwork_temp_dir, get_manifest_path, get_skills_dir, get_tools_dir,
    get_uv_binary_path, get_venvs_dir,
};
pub use types::{
    ArchiveFormat, ExecutablePaths, ExternalToolId, Platform, PlatformDownload, PlatformUrls,
    ToolDefinition, ToolStatus,
};
