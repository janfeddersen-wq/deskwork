//! External tool manager for coordinating downloads and installations.
//!
//! The `ExternalToolManager` is the main entry point for managing external tools.
//! It coordinates between the catalog, manifest, downloader, and extractor.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use super::catalog::{get_all_tool_definitions, get_tool_definition};
use super::downloader::{download_file, DownloadProgress};
use super::extractor::{extract_archive, make_executable};
use super::manifest::{load_manifest, save_manifest, ToolsManifest};
use super::paths;
use super::types::{ExternalToolId, Platform, ToolDefinition, ToolStatus};

// ============================================================================
// Tool Info
// ============================================================================

/// Combined information about a tool (definition + status).
#[derive(Debug, Clone)]
pub struct ToolInfo {
    /// The tool's static definition.
    pub definition: &'static ToolDefinition,
    /// Current installation status.
    pub status: ToolStatus,
    /// Path to the installed executable (if installed).
    pub executable_path: Option<PathBuf>,
}

// ============================================================================
// External Tool Manager
// ============================================================================

/// Manages external tool downloads, installations, and lifecycle.
///
/// This is the main entry point for the external tools system.
/// It's thread-safe and can be shared across async tasks.
pub struct ExternalToolManager {
    /// Base directory for all tools.
    tools_dir: PathBuf,
    /// Currently detected platform.
    platform: Option<Platform>,
    /// Cached manifest (protected by RwLock for async access).
    manifest: Arc<RwLock<ToolsManifest>>,
}

impl ExternalToolManager {
    /// Creates a new tool manager.
    ///
    /// This loads the manifest from disk and detects the current platform.
    /// Tools are stored in the OS temp directory under `deskwork/tools/`.
    pub fn new() -> Result<Self> {
        let tools_dir = paths::get_tools_dir();
        let platform = Platform::detect();

        // Ensure directories exist
        paths::ensure_dirs_exist()?;

        let manifest = load_manifest()?;

        info!(
            "ExternalToolManager initialized. Tools dir: {}, Platform: {:?}",
            tools_dir.display(),
            platform
        );

        Ok(Self {
            tools_dir,
            platform,
            manifest: Arc::new(RwLock::new(manifest)),
        })
    }

    /// Creates a tool manager with a custom tools directory (for testing).
    #[cfg(test)]
    pub fn with_tools_dir(tools_dir: PathBuf) -> Result<Self> {
        let platform = Platform::detect();

        Ok(Self {
            tools_dir,
            platform,
            manifest: Arc::new(RwLock::new(ToolsManifest::new())),
        })
    }

    /// Returns the base tools directory.
    pub fn tools_dir(&self) -> &Path {
        &self.tools_dir
    }

    /// Returns the detected platform.
    pub fn platform(&self) -> Option<Platform> {
        self.platform
    }

    // ========================================================================
    // Tool Queries
    // ========================================================================

    /// Lists all available tools with their current status.
    pub async fn list_tools(&self) -> Vec<ToolInfo> {
        let manifest = self.manifest.read().await;
        let platform = self.platform;

        get_all_tool_definitions()
            .into_iter()
            .map(|def| self.build_tool_info(def, &manifest, platform))
            .collect()
    }

    /// Gets the status of a specific tool.
    pub async fn status(&self, tool_id: ExternalToolId) -> ToolStatus {
        let manifest = self.manifest.read().await;
        self.get_tool_status(tool_id, &manifest)
    }

    /// Gets the path to a tool's executable, if installed.
    pub async fn get_executable_path(&self, tool_id: ExternalToolId) -> Option<PathBuf> {
        let platform = self.platform?;
        let manifest = self.manifest.read().await;
        if !manifest.is_installed(tool_id) {
            return None;
        }

        let def = get_tool_definition(tool_id);
        let tool_dir = self.get_tool_dir(tool_id);
        let exec_relpath = def.get_executable_path(platform);
        let exec_path = tool_dir.join(exec_relpath);

        if exec_path.exists() {
            Some(exec_path)
        } else {
            None
        }
    }

    /// Checks if UV is installed and returns its path, or installs it first.
    ///
    /// This is a convenience method that combines checking and installing.
    pub async fn ensure_uv_available(&self) -> Result<PathBuf> {
        if let Some(path) = self.get_executable_path(ExternalToolId::Uv).await {
            return Ok(path);
        }

        // UV not installed — install it
        info!("UV not found, installing...");
        self.install(ExternalToolId::Uv, |progress| {
            if let Some(percent) = progress.percent {
                debug!("UV download progress: {:.1}%", percent);
            }
        })
        .await?;

        self.get_executable_path(ExternalToolId::Uv)
            .await
            .ok_or_else(|| anyhow::anyhow!("UV installation completed but executable not found"))
    }

    // ========================================================================
    // Installation
    // ========================================================================

    /// Installs a tool with progress reporting.
    pub async fn install<F>(&self, tool_id: ExternalToolId, progress_cb: F) -> Result<()>
    where
        F: Fn(DownloadProgress) + Send + Sync,
    {
        let def = get_tool_definition(tool_id);

        let platform = self
            .platform
            .ok_or_else(|| anyhow::anyhow!("Cannot install tools: unsupported platform"))?;

        let download_info = def.get_download_for_platform(platform).ok_or_else(|| {
            anyhow::anyhow!("{} is not supported on {}", def.display_name, platform)
        })?;

        let url = download_info.url;
        let expected_sha256 = download_info.sha256;

        let format = def
            .get_archive_format(platform)
            .ok_or_else(|| anyhow::anyhow!("Unknown archive format for {}", def.display_name))?;

        info!(
            "Installing {} v{} from {}",
            def.display_name, def.version, url
        );

        let tool_dir = self.get_tool_dir(tool_id);
        let archive_path = self.tools_dir.join(format!("{}.archive", tool_id.as_str()));

        // Clean up any previous partial install
        if tool_dir.exists() {
            tokio::fs::remove_dir_all(&tool_dir)
                .await
                .with_context(|| format!("Failed to clean up {}", tool_dir.display()))?;
        }

        // Download with optional SHA256 verification
        let bytes_downloaded =
            download_file(url, &archive_path, expected_sha256, progress_cb).await?;

        // Handle AppImage specially - no extraction needed
        if !format.requires_extraction() {
            info!("Setting up AppImage for {}", def.display_name);
            tokio::fs::create_dir_all(&tool_dir)
                .await
                .with_context(|| format!("Failed to create {}", tool_dir.display()))?;

            // Move the downloaded file to be the executable
            let exec_path = tool_dir.join(def.get_executable_path(platform));
            tokio::fs::rename(&archive_path, &exec_path)
                .await
                .with_context(|| format!("Failed to move AppImage to {}", exec_path.display()))?;

            // Make it executable
            make_executable(&exec_path)?;
        } else {
            // Extract archive
            info!("Extracting {} to {}", def.display_name, tool_dir.display());
            extract_archive(&archive_path, &tool_dir, format)?;

            // Find and make executable the main binary
            self.setup_executable(tool_id, &tool_dir, def).await?;

            // Clean up archive
            if let Err(e) = tokio::fs::remove_file(&archive_path).await {
                warn!("Failed to clean up archive: {}", e);
            }
        }

        // Update manifest
        {
            let mut manifest = self.manifest.write().await;
            manifest.mark_installed(tool_id, def.version.to_string(), bytes_downloaded);
            save_manifest(&manifest)?;
        }

        info!(
            "{} v{} installed successfully",
            def.display_name, def.version
        );
        Ok(())
    }

    /// Uninstalls a tool.
    pub async fn uninstall(&self, tool_id: ExternalToolId) -> Result<()> {
        let def = get_tool_definition(tool_id);
        let tool_dir = self.get_tool_dir(tool_id);

        info!("Uninstalling {}", def.display_name);

        if tool_dir.exists() {
            tokio::fs::remove_dir_all(&tool_dir)
                .await
                .with_context(|| format!("Failed to remove {}", tool_dir.display()))?;
        }

        {
            let mut manifest = self.manifest.write().await;
            manifest.mark_uninstalled(tool_id);
            save_manifest(&manifest)?;
        }

        info!("{} uninstalled successfully", def.display_name);
        Ok(())
    }

    // ========================================================================
    // Helper Methods
    // ========================================================================

    fn get_tool_dir(&self, tool_id: ExternalToolId) -> PathBuf {
        self.tools_dir.join(tool_id.as_str())
    }

    fn get_tool_status(&self, tool_id: ExternalToolId, manifest: &ToolsManifest) -> ToolStatus {
        let platform = match self.platform {
            Some(p) => p,
            None => return ToolStatus::UnsupportedPlatform,
        };

        let def = get_tool_definition(tool_id);
        if def.get_url_for_platform(platform).is_none() {
            return ToolStatus::UnsupportedPlatform;
        }

        if let Some(info) = manifest.get_tool(tool_id) {
            let tool_dir = self.get_tool_dir(tool_id);
            let exec_relpath = def.get_executable_path(platform);
            let exec_path = tool_dir.join(exec_relpath);

            if exec_path.exists() {
                ToolStatus::Installed {
                    version: info.version.clone(),
                }
            } else {
                debug!(
                    "{} marked installed but executable not found at {}",
                    tool_id,
                    exec_path.display()
                );
                ToolStatus::NotInstalled
            }
        } else {
            ToolStatus::NotInstalled
        }
    }

    fn build_tool_info(
        &self,
        definition: &'static ToolDefinition,
        manifest: &ToolsManifest,
        platform: Option<Platform>,
    ) -> ToolInfo {
        let status = match platform {
            Some(p) if definition.get_url_for_platform(p).is_some() => {
                if let Some(info) = manifest.get_tool(definition.id) {
                    let tool_dir = self.get_tool_dir(definition.id);
                    let exec_relpath = definition.get_executable_path(p);
                    let exec_path = tool_dir.join(exec_relpath);

                    if exec_path.exists() {
                        ToolStatus::Installed {
                            version: info.version.clone(),
                        }
                    } else {
                        ToolStatus::NotInstalled
                    }
                } else {
                    ToolStatus::NotInstalled
                }
            }
            _ => ToolStatus::UnsupportedPlatform,
        };

        let executable_path = if let (ToolStatus::Installed { .. }, Some(p)) = (&status, platform) {
            let tool_dir = self.get_tool_dir(definition.id);
            let exec_relpath = definition.get_executable_path(p);
            Some(tool_dir.join(exec_relpath))
        } else {
            None
        };

        ToolInfo {
            definition,
            status,
            executable_path,
        }
    }

    /// Sets up the executable after extraction.
    async fn setup_executable(
        &self,
        tool_id: ExternalToolId,
        tool_dir: &Path,
        def: &ToolDefinition,
    ) -> Result<()> {
        let platform = self
            .platform
            .ok_or_else(|| anyhow::anyhow!("Cannot setup executable: unsupported platform"))?;

        let exec_relpath = def.get_executable_path(platform);
        let exec_name = Path::new(exec_relpath)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(tool_id.as_str());

        // First, check if it exists at the expected path
        let expected_path = tool_dir.join(exec_relpath);
        if expected_path.exists() {
            make_executable(&expected_path)?;
            return Ok(());
        }

        // Otherwise, look for it in subdirectories
        if let Ok(mut entries) = tokio::fs::read_dir(tool_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let entry_path = entry.path();
                if entry_path.is_dir() {
                    let nested_exec = entry_path.join(exec_relpath);
                    if nested_exec.exists() {
                        debug!(
                            "Found executable in subdirectory, relocating from {}",
                            entry_path.display()
                        );
                        self.flatten_directory(&entry_path, tool_dir).await?;

                        let final_path = tool_dir.join(exec_relpath);
                        if final_path.exists() {
                            make_executable(&final_path)?;
                            return Ok(());
                        }
                    }

                    let alt_path = entry_path.join(exec_name);
                    if alt_path.exists() {
                        make_executable(&alt_path)?;
                        return Ok(());
                    }
                }
            }
        }

        // Last resort: search recursively
        if let Some(found) = self.find_executable_recursive(tool_dir, exec_name).await? {
            make_executable(&found)?;
            info!("Found {} at {}", exec_name, found.display());
            return Ok(());
        }

        anyhow::bail!(
            "Could not find {} executable in extracted files",
            def.display_name
        )
    }

    async fn flatten_directory(&self, from: &Path, to: &Path) -> Result<()> {
        let mut entries = tokio::fs::read_dir(from).await?;

        while let Some(entry) = entries.next_entry().await? {
            let source = entry.path();
            let dest = to.join(entry.file_name());

            if dest.exists() {
                continue;
            }

            tokio::fs::rename(&source, &dest).await.with_context(|| {
                format!("Failed to move {} to {}", source.display(), dest.display())
            })?;
        }

        tokio::fs::remove_dir(from).await.ok();
        Ok(())
    }

    async fn find_executable_recursive(&self, dir: &Path, name: &str) -> Result<Option<PathBuf>> {
        let mut stack = vec![dir.to_path_buf()];

        while let Some(current) = stack.pop() {
            if let Ok(mut entries) = tokio::fs::read_dir(&current).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let path = entry.path();
                    if path.is_dir() {
                        stack.push(path);
                    } else if path.file_name().and_then(|s| s.to_str()) == Some(name) {
                        return Ok(Some(path));
                    }
                }
            }
        }

        Ok(None)
    }
}

// ============================================================================
// FUSE Availability Check (Linux only)
// ============================================================================

/// URL to the FUSE installation documentation.
pub const FUSE_DOCS_URL: &str =
    "https://github.com/janfeddersen-wq/deskwork/blob/main/docs/fuse-installation.md";

/// Checks if FUSE is available on the system (required for AppImage).
#[cfg(target_os = "linux")]
pub fn is_fuse_available() -> bool {
    use std::path::Path;
    let fuse_paths = [
        "/usr/lib/libfuse.so.2",
        "/usr/lib64/libfuse.so.2",
        "/usr/lib/x86_64-linux-gnu/libfuse.so.2",
        "/lib/x86_64-linux-gnu/libfuse.so.2",
        "/usr/lib/aarch64-linux-gnu/libfuse.so.2",
        "/lib/aarch64-linux-gnu/libfuse.so.2",
    ];
    fuse_paths.iter().any(|p| Path::new(p).exists())
}

#[cfg(not(target_os = "linux"))]
pub fn is_fuse_available() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_manager_list_tools() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ExternalToolManager::with_tools_dir(temp_dir.path().to_path_buf()).unwrap();

        let tools = manager.list_tools().await;
        assert_eq!(tools.len(), 4);

        for tool in &tools {
            assert!(
                matches!(
                    tool.status,
                    ToolStatus::NotInstalled | ToolStatus::UnsupportedPlatform
                ),
                "Tool {:?} has unexpected status: {:?}",
                tool.definition.id,
                tool.status
            );
        }
    }

    #[tokio::test]
    async fn test_manager_status() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ExternalToolManager::with_tools_dir(temp_dir.path().to_path_buf()).unwrap();

        let status = manager.status(ExternalToolId::Pandoc).await;
        assert!(
            matches!(
                status,
                ToolStatus::NotInstalled | ToolStatus::UnsupportedPlatform
            ),
            "Unexpected status: {:?}",
            status
        );
    }

    #[tokio::test]
    async fn test_manager_get_executable_path_not_installed() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ExternalToolManager::with_tools_dir(temp_dir.path().to_path_buf()).unwrap();

        let path = manager.get_executable_path(ExternalToolId::Uv).await;
        assert!(path.is_none());
    }

    #[test]
    fn test_get_tool_dir() {
        let temp_dir = TempDir::new().unwrap();
        let manager = ExternalToolManager::with_tools_dir(temp_dir.path().to_path_buf()).unwrap();

        let pandoc_dir = manager.get_tool_dir(ExternalToolId::Pandoc);
        assert!(pandoc_dir.ends_with("pandoc"));

        let node_dir = manager.get_tool_dir(ExternalToolId::Node);
        assert!(node_dir.ends_with("node"));
    }
}
