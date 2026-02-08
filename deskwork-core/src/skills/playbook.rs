//! Playbook file management for skill categories.
//!
//! Each category can optionally have a "playbook" — a user-customizable
//! configuration file that defines organization-specific settings (e.g.,
//! contract review positions, risk tolerances, templates).
//!
//! Playbooks are stored in the application's data directory under a
//! `playbooks/` subdirectory, using the naming convention `{category_id}.local.md`.
//!
//! Path: `{data_dir}/deskwork/playbooks/{category_id}.local.md`

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info};

/// Subdirectory under the deskwork data dir where playbooks are stored.
const PLAYBOOKS_DIR: &str = "playbooks";

/// Returns the deskwork data directory (same base as the DB).
///
/// e.g. `~/.local/share/deskwork/` on Linux, `~/Library/Application Support/deskwork/` on macOS
fn get_deskwork_data_dir() -> Result<PathBuf> {
    let data_dir = dirs::data_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join(".local/share")))
        .ok_or_else(|| anyhow::anyhow!("Could not determine data directory"))?;

    Ok(data_dir.join("deskwork"))
}

/// Returns the playbooks directory.
///
/// Path: `{data_dir}/deskwork/playbooks/`
pub fn get_playbooks_dir() -> Result<PathBuf> {
    Ok(get_deskwork_data_dir()?.join(PLAYBOOKS_DIR))
}

/// Returns the expected file name for a category's playbook.
///
/// Example: `"legal"` → `"legal.local.md"`
pub fn playbook_file_name(category_id: &str) -> String {
    format!("{category_id}.local.md")
}

/// Returns the full path where a category's playbook file should live.
///
/// Path: `{data_dir}/deskwork/playbooks/{category_id}.local.md`
pub fn get_playbook_path(category_id: &str) -> Result<PathBuf> {
    Ok(get_playbooks_dir()?.join(playbook_file_name(category_id)))
}

/// Write playbook content to disk at the expected path.
///
/// Creates the `playbooks/` directory if it doesn't exist.
pub fn write_playbook_to_disk(category_id: &str, content: &str) -> Result<()> {
    let path = get_playbook_path(category_id)?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory: {}", parent.display()))?;
    }

    fs::write(&path, content)
        .with_context(|| format!("Failed to write playbook file: {}", path.display()))?;

    info!(
        "Wrote playbook for category '{}' to {}",
        category_id,
        path.display()
    );

    Ok(())
}

/// Read a category's playbook from disk, if it exists.
///
/// Returns `None` if the file doesn't exist or the data directory can't be resolved.
pub fn read_playbook_from_disk(category_id: &str) -> Option<String> {
    let path = get_playbook_path(category_id).ok()?;

    match fs::read_to_string(&path) {
        Ok(content) => {
            debug!(
                "Read playbook for category '{}' from {}",
                category_id,
                path.display()
            );
            Some(content)
        }
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_playbook_file_name() {
        assert_eq!(playbook_file_name("legal"), "legal.local.md");
        assert_eq!(playbook_file_name("finance"), "finance.local.md");
    }

    #[test]
    fn test_get_playbook_path_contains_expected_components() {
        let path = get_playbook_path("legal").unwrap();
        let path_str = path.to_string_lossy();
        assert!(
            path_str.contains("deskwork"),
            "Path should contain 'deskwork': {path_str}"
        );
        assert!(
            path_str.contains("playbooks"),
            "Path should contain 'playbooks': {path_str}"
        );
        assert!(
            path_str.ends_with("legal.local.md"),
            "Path should end with 'legal.local.md': {path_str}"
        );
    }

    #[test]
    fn test_get_playbooks_dir_is_under_deskwork() {
        let dir = get_playbooks_dir().unwrap();
        let dir_str = dir.to_string_lossy();
        assert!(dir_str.contains("deskwork"));
        assert!(dir_str.ends_with("playbooks"));
    }

    #[test]
    fn test_write_and_read_roundtrip() {
        // This writes to the real data dir — acceptable for integration tests
        let test_id = "test-playbook-roundtrip";
        let content = "# Test Playbook\n\nRoundtrip test content.";

        write_playbook_to_disk(test_id, content).unwrap();

        let read_back = read_playbook_from_disk(test_id);
        assert_eq!(read_back, Some(content.to_string()));

        // Clean up
        if let Ok(path) = get_playbook_path(test_id) {
            let _ = fs::remove_file(path);
        }
    }

    #[test]
    fn test_read_nonexistent_returns_none() {
        assert_eq!(read_playbook_from_disk("nonexistent-category-xyz"), None);
    }
}
