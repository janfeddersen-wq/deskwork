//! Tool manifest persistence.
//!
//! This module handles reading and writing the `manifest.json` file that tracks
//! installed tools, their versions, and installation metadata.
//!
//! The manifest is stored at `{temp}/deskwork/tools/manifest.json`.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tracing::{debug, info, warn};

use super::paths;
use super::types::ExternalToolId;

/// Current schema version for the manifest file.
const SCHEMA_VERSION: u32 = 1;

// ============================================================================
// Manifest Data Structures
// ============================================================================

/// Information about a single installed tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledToolInfo {
    /// Installed version string.
    pub version: String,
    /// When the tool was installed.
    pub installed_at: DateTime<Utc>,
    /// Size of the installed tool in bytes.
    pub size_bytes: u64,
    /// Whether the tool is enabled for use.
    #[serde(default = "default_enabled")]
    pub enabled: bool,
}

fn default_enabled() -> bool {
    true
}

/// Root structure for the tools manifest file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsManifest {
    /// Schema version for forward compatibility.
    pub schema_version: u32,
    /// Map of tool ID to installation info.
    #[serde(default)]
    pub tools: HashMap<String, InstalledToolInfo>,
}

impl Default for ToolsManifest {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            tools: HashMap::new(),
        }
    }
}

impl ToolsManifest {
    /// Creates a new empty manifest.
    pub fn new() -> Self {
        Self::default()
    }

    /// Checks if a tool is installed.
    pub fn is_installed(&self, tool_id: ExternalToolId) -> bool {
        self.tools.contains_key(tool_id.as_str())
    }

    /// Gets installation info for a tool.
    pub fn get_tool(&self, tool_id: ExternalToolId) -> Option<&InstalledToolInfo> {
        self.tools.get(tool_id.as_str())
    }

    /// Records a tool as installed.
    pub fn mark_installed(&mut self, tool_id: ExternalToolId, version: String, size_bytes: u64) {
        let info = InstalledToolInfo {
            version,
            installed_at: Utc::now(),
            size_bytes,
            enabled: true,
        };
        self.tools.insert(tool_id.as_str().to_string(), info);
    }

    /// Removes a tool from the manifest.
    pub fn mark_uninstalled(&mut self, tool_id: ExternalToolId) {
        self.tools.remove(tool_id.as_str());
    }

    /// Returns a list of all installed tool IDs.
    pub fn installed_tools(&self) -> Vec<ExternalToolId> {
        self.tools.keys().filter_map(|k| k.parse().ok()).collect()
    }

    /// Calculates total size of all installed tools in bytes.
    pub fn total_size_bytes(&self) -> u64 {
        self.tools.values().map(|info| info.size_bytes).sum()
    }
}

// ============================================================================
// Manifest Persistence
// ============================================================================

/// Loads the tools manifest from disk.
///
/// If the manifest doesn't exist, returns a new empty manifest.
/// If the manifest exists but is corrupted, logs a warning and returns empty.
pub fn load_manifest() -> Result<ToolsManifest> {
    load_manifest_from(&paths::get_manifest_path())
}

/// Loads the manifest from a specific path (for testing).
pub fn load_manifest_from(path: &Path) -> Result<ToolsManifest> {
    if !path.exists() {
        debug!("Manifest not found at {}, creating new", path.display());
        return Ok(ToolsManifest::new());
    }

    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read manifest from {}", path.display()))?;

    match serde_json::from_str::<ToolsManifest>(&content) {
        Ok(mut manifest) => {
            if manifest.schema_version != SCHEMA_VERSION {
                info!(
                    "Manifest schema version {} differs from current {}, migrating",
                    manifest.schema_version, SCHEMA_VERSION
                );
                manifest.schema_version = SCHEMA_VERSION;
            }
            Ok(manifest)
        }
        Err(e) => {
            warn!(
                "Failed to parse manifest at {}: {}. Starting fresh.",
                path.display(),
                e
            );
            Ok(ToolsManifest::new())
        }
    }
}

/// Saves the tools manifest to disk.
pub fn save_manifest(manifest: &ToolsManifest) -> Result<()> {
    save_manifest_to(manifest, &paths::get_manifest_path())
}

/// Saves the manifest to a specific path (for testing).
pub fn save_manifest_to(manifest: &ToolsManifest, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create manifest directory: {}", parent.display())
        })?;
    }

    let content = serde_json::to_string_pretty(manifest).context("Failed to serialize manifest")?;

    fs::write(path, content)
        .with_context(|| format!("Failed to write manifest to {}", path.display()))?;

    debug!("Manifest saved to {}", path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_manifest() -> ToolsManifest {
        let mut manifest = ToolsManifest::new();
        manifest.mark_installed(ExternalToolId::Uv, "0.5.14".to_string(), 15_000_000);
        manifest
    }

    #[test]
    fn test_manifest_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("manifest.json");

        let manifest = create_test_manifest();
        save_manifest_to(&manifest, &manifest_path).unwrap();

        let loaded = load_manifest_from(&manifest_path).unwrap();
        assert_eq!(loaded.schema_version, SCHEMA_VERSION);
        assert!(loaded.is_installed(ExternalToolId::Uv));

        let uv_info = loaded.get_tool(ExternalToolId::Uv).unwrap();
        assert_eq!(uv_info.version, "0.5.14");
        assert_eq!(uv_info.size_bytes, 15_000_000);
        assert!(uv_info.enabled);
    }

    #[test]
    fn test_manifest_missing_file_returns_empty() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("nonexistent").join("manifest.json");

        let manifest = load_manifest_from(&manifest_path).unwrap();
        assert!(manifest.tools.is_empty());
        assert_eq!(manifest.schema_version, SCHEMA_VERSION);
    }

    #[test]
    fn test_manifest_corrupted_returns_empty() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("manifest.json");

        fs::write(&manifest_path, "not valid json {{{{").unwrap();

        let manifest = load_manifest_from(&manifest_path).unwrap();
        assert!(manifest.tools.is_empty());
    }

    #[test]
    fn test_mark_installed_and_uninstalled() {
        let mut manifest = ToolsManifest::new();

        assert!(!manifest.is_installed(ExternalToolId::Uv));

        manifest.mark_installed(ExternalToolId::Uv, "0.5.14".to_string(), 15_000_000);
        assert!(manifest.is_installed(ExternalToolId::Uv));
        assert_eq!(
            manifest.get_tool(ExternalToolId::Uv).unwrap().version,
            "0.5.14"
        );

        manifest.mark_uninstalled(ExternalToolId::Uv);
        assert!(!manifest.is_installed(ExternalToolId::Uv));
    }

    #[test]
    fn test_installed_tools_list() {
        let mut manifest = ToolsManifest::new();
        manifest.mark_installed(ExternalToolId::Uv, "0.5.14".to_string(), 15_000_000);

        let installed = manifest.installed_tools();
        assert_eq!(installed.len(), 1);
        assert!(installed.contains(&ExternalToolId::Uv));
    }

    #[test]
    fn test_total_size_bytes() {
        let mut manifest = ToolsManifest::new();
        assert_eq!(manifest.total_size_bytes(), 0);

        manifest.mark_installed(ExternalToolId::Uv, "0.5.14".to_string(), 15_000_000);
        assert_eq!(manifest.total_size_bytes(), 15_000_000);
    }

    #[test]
    fn test_manifest_json_format() {
        let manifest = create_test_manifest();
        let json = serde_json::to_string_pretty(&manifest).unwrap();

        assert!(json.contains("schema_version"));
        assert!(json.contains("tools"));
        assert!(json.contains("uv"));
        assert!(json.contains("version"));
        assert!(json.contains("installed_at"));
        assert!(json.contains("size_bytes"));
        assert!(json.contains("enabled"));
    }
}
