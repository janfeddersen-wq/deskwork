//! Skills bundle management and discovery.
//!
//! This module handles:
//! - Extracting the embedded skills bundle to the data directory
//! - Discovering available skills and parsing their metadata
//!
//! Skills are Python scripts and other assets that extend Deskwork's capabilities.
//! The skills bundle is embedded at compile time and extracted on first run or
//! when the bundle version changes.
//!
//! Each skill lives in its own subdirectory under `{temp}/deskwork/skills/` and
//! contains a `SKILL.md` file with YAML frontmatter describing the skill.

pub mod categories;
pub mod category_context;
pub mod commands;
pub mod context;
pub mod discovery;
pub mod types;

pub use context::SkillsContext;
pub use discovery::{discover_skills, parse_skill_frontmatter, SkillMetadata};

use anyhow::{Context, Result};
use std::fs;
use std::io::Cursor;
use std::path::PathBuf;
use tracing::{debug, info, warn};
use zip::ZipArchive;

use crate::external_tools::paths::get_skills_dir;

/// The embedded skills bundle (skills.zip from project root).
static SKILLS_BUNDLE: &[u8] = include_bytes!("../../../skills.zip");

/// Version file name used to track the current bundle version.
const VERSION_FILE: &str = ".version";

/// Computes a simple version marker for the embedded bundle.
fn compute_bundle_version() -> String {
    let len = SKILLS_BUNDLE.len();
    let sample_size = std::cmp::min(1024, len);
    let sample = &SKILLS_BUNDLE[..sample_size];

    let checksum: u64 = sample
        .iter()
        .enumerate()
        .map(|(i, &b)| (b as u64).wrapping_mul((i + 1) as u64))
        .fold(0u64, |acc, x| acc.wrapping_add(x));

    format!("v1-{}-{:016x}", len, checksum)
}

/// Extracts the skills bundle if needed and returns the skills directory path.
pub fn extract_skills_if_needed() -> Result<PathBuf> {
    let skills_dir = get_skills_dir();
    let version_file = skills_dir.join(VERSION_FILE);
    let current_version = compute_bundle_version();

    let needs_extraction = if version_file.exists() {
        match fs::read_to_string(&version_file) {
            Ok(existing_version) => {
                let existing = existing_version.trim();
                if existing == current_version {
                    debug!("Skills bundle up-to-date (version: {})", current_version);
                    false
                } else {
                    info!(
                        "Skills bundle version changed: {} -> {}",
                        existing, current_version
                    );
                    true
                }
            }
            Err(e) => {
                warn!("Failed to read version file, will re-extract: {}", e);
                true
            }
        }
    } else {
        info!("Skills bundle not found, extracting...");
        true
    };

    if needs_extraction {
        extract_bundle(&skills_dir)?;
        fs::write(&version_file, &current_version)
            .with_context(|| format!("Failed to write version file: {}", version_file.display()))?;
        info!(
            "Skills bundle extracted to {} (version: {})",
            skills_dir.display(),
            current_version
        );
    }

    Ok(skills_dir)
}

/// Extracts the embedded ZIP bundle to the target directory.
fn extract_bundle(target_dir: &PathBuf) -> Result<()> {
    fs::create_dir_all(target_dir).with_context(|| {
        format!(
            "Failed to create skills directory: {}",
            target_dir.display()
        )
    })?;

    let cursor = Cursor::new(SKILLS_BUNDLE);
    let mut archive = ZipArchive::new(cursor).context("Failed to read skills bundle as ZIP")?;

    debug!("Extracting {} files from skills bundle", archive.len());

    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .with_context(|| format!("Failed to read ZIP entry {}", i))?;

        if file.is_symlink() {
            warn!(
                "Skipping symlink in ZIP archive: {:?} (security restriction)",
                file.name()
            );
            continue;
        }

        let outpath = match file.enclosed_name() {
            Some(path) => target_dir.join(path),
            None => {
                warn!("Skipping ZIP entry with unsafe path: {:?}", file.name());
                continue;
            }
        };

        if file.is_dir() {
            fs::create_dir_all(&outpath)
                .with_context(|| format!("Failed to create directory: {}", outpath.display()))?;
        } else {
            if let Some(parent) = outpath.parent() {
                if !parent.exists() {
                    fs::create_dir_all(parent).with_context(|| {
                        format!("Failed to create parent directory: {}", parent.display())
                    })?;
                }
            }

            let mut outfile = fs::File::create(&outpath)
                .with_context(|| format!("Failed to create file: {}", outpath.display()))?;

            std::io::copy(&mut file, &mut outfile)
                .with_context(|| format!("Failed to write file: {}", outpath.display()))?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Some(mode) = file.unix_mode() {
                    let sanitized_mode = (mode & 0o0777) & 0o0755;
                    fs::set_permissions(&outpath, fs::Permissions::from_mode(sanitized_mode)).ok();
                }
            }
        }
    }

    Ok(())
}

/// Returns the path to a specific skill file.
pub fn get_skill_path(relative_path: &str) -> PathBuf {
    get_skills_dir().join(relative_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_bundle_version_is_stable() {
        let v1 = compute_bundle_version();
        let v2 = compute_bundle_version();
        assert_eq!(v1, v2, "Version should be deterministic");
    }

    #[test]
    fn test_compute_bundle_version_format() {
        let version = compute_bundle_version();
        assert!(version.starts_with("v1-"), "Version should start with v1-");
        assert!(
            version.len() > 20,
            "Version should include length and checksum"
        );
    }

    #[test]
    fn test_skills_bundle_is_valid_zip() {
        let cursor = Cursor::new(SKILLS_BUNDLE);
        let archive = ZipArchive::new(cursor);
        assert!(archive.is_ok(), "Embedded bundle should be valid ZIP");
        let archive = archive.unwrap();
        assert!(!archive.is_empty(), "Bundle should contain files");
    }
}
