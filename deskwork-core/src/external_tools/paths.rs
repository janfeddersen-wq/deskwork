//! Temp-folder path management for external tools.
//!
//! All external tools (UV, venvs, etc.) are stored under the OS temp directory:
//!
//! - Linux: `/tmp/deskwork/`
//! - macOS: `/var/folders/.../deskwork/` (per-user temp via std::env::temp_dir)
//! - Windows: `C:\Users\<User>\AppData\Local\Temp\deskwork\`
//!
//! This keeps the main application data directory clean and uses the OS-provided
//! temp location which is always available on every platform.

use anyhow::{Context, Result};
use std::path::PathBuf;

/// Subdirectory name under the OS temp folder.
const DESKWORK_TEMP_DIR: &str = "deskwork";

// ============================================================================
// Path Resolution
// ============================================================================

/// Returns the base deskwork directory inside the OS temp folder.
///
/// e.g., `/tmp/deskwork/` on Linux, `/var/folders/.../deskwork/` on macOS
pub fn get_deskwork_temp_dir() -> PathBuf {
    std::env::temp_dir().join(DESKWORK_TEMP_DIR)
}

/// Returns the path to the external tools directory.
///
/// Path: `{temp}/deskwork/tools/`
pub fn get_tools_dir() -> PathBuf {
    get_deskwork_temp_dir().join("tools")
}

/// Returns the path to the tools manifest file.
///
/// Path: `{temp}/deskwork/tools/manifest.json`
pub fn get_manifest_path() -> PathBuf {
    get_tools_dir().join("manifest.json")
}

/// Returns the path to the virtual environments directory.
///
/// Path: `{temp}/deskwork/venvs/`
pub fn get_venvs_dir() -> PathBuf {
    get_deskwork_temp_dir().join("venvs")
}

/// Returns the path to the skills directory.
///
/// Path: `{temp}/deskwork/skills/`
pub fn get_skills_dir() -> PathBuf {
    get_deskwork_temp_dir().join("skills")
}

/// Returns the platform-specific path to the UV binary.
///
/// - Linux/macOS: `{temp}/deskwork/tools/uv/uv`
/// - Windows: `{temp}/deskwork/tools/uv/uv.exe`
pub fn get_uv_binary_path() -> PathBuf {
    let tools_dir = get_tools_dir();

    #[cfg(windows)]
    let uv_name = "uv.exe";

    #[cfg(not(windows))]
    let uv_name = "uv";

    tools_dir.join("uv").join(uv_name)
}

/// Ensures all required directories exist under the temp folder.
///
/// Creates:
/// - `{temp}/deskwork/`
/// - `{temp}/deskwork/tools/`
/// - `{temp}/deskwork/venvs/`
///
/// # Errors
///
/// Returns an error if any directory cannot be created (e.g., permission issues).
pub fn ensure_dirs_exist() -> Result<()> {
    let dirs = [
        get_deskwork_temp_dir(),
        get_tools_dir(),
        get_venvs_dir(),
        get_skills_dir(),
    ];

    for dir in dirs {
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("Failed to create directory: {}", dir.display()))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deskwork_temp_dir_contains_deskwork() {
        let dir = get_deskwork_temp_dir();
        let dir_str = dir.to_string_lossy();
        assert!(
            dir_str.contains("deskwork"),
            "Temp dir should contain 'deskwork': {}",
            dir_str
        );
    }

    #[test]
    fn test_tools_dir_is_under_temp() {
        let temp = get_deskwork_temp_dir();
        let tools = get_tools_dir();
        assert!(tools.starts_with(&temp));
        assert!(tools.ends_with("tools"));
    }

    #[test]
    fn test_venvs_dir_is_under_temp() {
        let temp = get_deskwork_temp_dir();
        let venvs = get_venvs_dir();
        assert!(venvs.starts_with(&temp));
        assert!(venvs.ends_with("venvs"));
    }

    #[test]
    fn test_manifest_path_is_under_tools() {
        let tools = get_tools_dir();
        let manifest = get_manifest_path();
        assert!(manifest.starts_with(&tools));
        assert!(manifest.ends_with("manifest.json"));
    }

    #[test]
    fn test_uv_binary_path() {
        let tools = get_tools_dir();
        let uv = get_uv_binary_path();
        assert!(uv.starts_with(&tools));

        #[cfg(windows)]
        assert!(uv.to_string_lossy().ends_with("uv.exe"));

        #[cfg(not(windows))]
        {
            let uv_str = uv.to_string_lossy();
            assert!(
                uv_str.ends_with("uv/uv"),
                "UV path should end with uv/uv, got: {}",
                uv_str
            );
        }
    }

    #[test]
    fn test_skills_dir_is_under_temp() {
        let temp = get_deskwork_temp_dir();
        let skills = get_skills_dir();
        assert!(skills.starts_with(&temp));
        assert!(skills.ends_with("skills"));
    }

    #[test]
    fn test_ensure_dirs_exist() {
        // This actually creates dirs in the real temp folder
        // but that's fine for tests — temp is meant for this
        ensure_dirs_exist().expect("Should be able to create dirs in temp");

        assert!(get_deskwork_temp_dir().exists());
        assert!(get_tools_dir().exists());
        assert!(get_venvs_dir().exists());
        assert!(get_skills_dir().exists());
    }
}
