//! Archive extraction for downloaded tool packages.
//!
//! This module handles extracting various archive formats (zip, tar.gz)
//! and setting executable permissions on Unix systems.

use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Write};
use std::path::Path;
use tracing::{debug, info, warn};

use super::types::ArchiveFormat;

// ============================================================================
// Archive Extraction
// ============================================================================

/// Extracts an archive to a destination directory.
///
/// # Arguments
///
/// * `archive_path` - Path to the archive file.
/// * `dest_dir` - Directory to extract into.
/// * `format` - The archive format.
///
/// # Errors
///
/// Returns an error if extraction fails.
pub fn extract_archive(archive_path: &Path, dest_dir: &Path, format: ArchiveFormat) -> Result<()> {
    info!(
        "Extracting {:?} archive {} to {}",
        format,
        archive_path.display(),
        dest_dir.display()
    );

    // Ensure destination directory exists
    fs::create_dir_all(dest_dir)
        .with_context(|| format!("Failed to create directory: {}", dest_dir.display()))?;

    match format {
        ArchiveFormat::Zip => extract_zip(archive_path, dest_dir),
        ArchiveFormat::TarGz => extract_tar_gz(archive_path, dest_dir),
        ArchiveFormat::TarXz => extract_tar_xz(archive_path, dest_dir),
        ArchiveFormat::AppImage => {
            anyhow::bail!(
                "AppImage is not an extractable archive; it should be handled by the manager"
            )
        }
    }
}

// ============================================================================
// ZIP Extraction
// ============================================================================

fn extract_zip(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let file = File::open(archive_path)
        .with_context(|| format!("Failed to open zip: {}", archive_path.display()))?;

    let mut archive = zip::ZipArchive::new(file)
        .with_context(|| format!("Failed to read zip: {}", archive_path.display()))?;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let entry_path = match entry.enclosed_name() {
            Some(path) => path.to_owned(),
            None => {
                debug!("Skipping unsafe path in zip");
                continue;
            }
        };

        let dest_path = dest_dir.join(&entry_path);

        if entry.is_dir() {
            fs::create_dir_all(&dest_path)?;
        } else {
            // Ensure parent directory exists
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }

            let mut outfile = File::create(&dest_path)
                .with_context(|| format!("Failed to create: {}", dest_path.display()))?;

            io::copy(&mut entry, &mut outfile)?;

            // Set executable permissions on Unix
            #[cfg(unix)]
            set_unix_permissions(&dest_path, entry.unix_mode())?;
        }
    }

    debug!("ZIP extraction complete");
    Ok(())
}

// ============================================================================
// TAR.GZ Extraction
// ============================================================================

fn extract_tar_gz(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let file = File::open(archive_path)
        .with_context(|| format!("Failed to open tar.gz: {}", archive_path.display()))?;

    let reader = BufReader::new(file);
    let decoder = flate2::read::GzDecoder::new(reader);
    extract_tar(decoder, dest_dir)
}

fn extract_tar_xz(archive_path: &Path, dest_dir: &Path) -> Result<()> {
    let file = File::open(archive_path)
        .with_context(|| format!("Failed to open tar.xz: {}", archive_path.display()))?;

    let reader = BufReader::new(file);
    let decoder = xz2::read::XzDecoder::new(reader);
    extract_tar(decoder, dest_dir)
}

// ============================================================================
// Common TAR Extraction
// ============================================================================

fn extract_tar<R: Read>(reader: R, dest_dir: &Path) -> Result<()> {
    let mut archive = tar::Archive::new(reader);
    let dest_dir_canonical = dest_dir
        .canonicalize()
        .unwrap_or_else(|_| dest_dir.to_path_buf());

    for entry_result in archive.entries()? {
        let mut entry = entry_result?;
        let entry_type = entry.header().entry_type();

        // Security: Skip symlinks and hardlinks entirely to prevent escape attacks
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            warn!("Skipping symlink/hardlink in tar archive (security)");
            continue;
        }

        let path = entry.path()?;

        // Security: skip absolute paths and paths with ..
        if path.is_absolute()
            || path
                .components()
                .any(|c| c == std::path::Component::ParentDir)
        {
            warn!("Skipping unsafe path in tar: {:?}", path);
            continue;
        }

        let dest_path = dest_dir.join(&path);

        // Security: Verify destination is within dest_dir
        let dest_canonical = if dest_path.exists() {
            dest_path.canonicalize()?
        } else if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
            let parent_canonical = parent.canonicalize()?;
            parent_canonical.join(dest_path.file_name().unwrap_or_default())
        } else {
            dest_path.clone()
        };

        if !dest_canonical.starts_with(&dest_dir_canonical) {
            warn!(
                "Skipping path that escapes dest_dir: {:?} -> {:?}",
                path, dest_canonical
            );
            continue;
        }

        if entry_type.is_dir() {
            fs::create_dir_all(&dest_path)?;
        } else if entry_type.is_file() {
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }

            let mut outfile = File::create(&dest_path)
                .with_context(|| format!("Failed to create: {}", dest_path.display()))?;
            io::copy(&mut entry, &mut outfile)?;
            outfile.flush()?;

            #[cfg(unix)]
            {
                if let Ok(mode) = entry.header().mode() {
                    set_unix_permissions(&dest_path, Some(mode))?;
                }
            }
        }
    }

    debug!("TAR extraction complete");
    Ok(())
}

// ============================================================================
// Unix Permissions
// ============================================================================

#[cfg(unix)]
fn set_unix_permissions(path: &Path, mode: Option<u32>) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    if let Some(mode) = mode {
        if mode & 0o111 != 0 {
            let permissions = fs::Permissions::from_mode(mode | 0o755);
            fs::set_permissions(path, permissions)
                .with_context(|| format!("Failed to set permissions on {}", path.display()))?;
        }
    }

    Ok(())
}

/// Sets executable permission on a file (Unix only).
///
/// On Windows, this is a no-op.
#[allow(unused_variables)]
pub fn make_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let metadata = fs::metadata(path)
            .with_context(|| format!("Failed to get metadata for {}", path.display()))?;

        let mut permissions = metadata.permissions();
        let current_mode = permissions.mode();
        permissions.set_mode(current_mode | 0o755);

        fs::set_permissions(path, permissions).with_context(|| {
            format!("Failed to set executable permission on {}", path.display())
        })?;

        debug!("Set executable permission on {}", path.display());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_extract_zip_simple() {
        let temp_dir = TempDir::new().unwrap();
        let archive_path = temp_dir.path().join("test.zip");
        let extract_dir = temp_dir.path().join("extracted");

        // Create a simple zip file
        {
            let file = File::create(&archive_path).unwrap();
            let mut zip = zip::ZipWriter::new(file);

            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);

            zip.start_file("hello.txt", options).unwrap();
            zip.write_all(b"Hello, World!").unwrap();

            zip.start_file("subdir/nested.txt", options).unwrap();
            zip.write_all(b"Nested content").unwrap();

            zip.finish().unwrap();
        }

        extract_archive(&archive_path, &extract_dir, ArchiveFormat::Zip).unwrap();

        assert!(extract_dir.join("hello.txt").exists());
        assert!(extract_dir.join("subdir/nested.txt").exists());

        let content = fs::read_to_string(extract_dir.join("hello.txt")).unwrap();
        assert_eq!(content, "Hello, World!");
    }

    #[test]
    fn test_extract_tar_gz_simple() {
        let temp_dir = TempDir::new().unwrap();
        let archive_path = temp_dir.path().join("test.tar.gz");
        let extract_dir = temp_dir.path().join("extracted");

        // Create a simple tar.gz file
        {
            let file = File::create(&archive_path).unwrap();
            let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut builder = tar::Builder::new(encoder);

            let data = b"Hello from tar.gz!";
            let mut header = tar::Header::new_gnu();
            header.set_path("greetings.txt").unwrap();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();

            builder.append(&header, &data[..]).unwrap();
            builder.finish().unwrap();
        }

        extract_archive(&archive_path, &extract_dir, ArchiveFormat::TarGz).unwrap();

        assert!(extract_dir.join("greetings.txt").exists());
        let content = fs::read_to_string(extract_dir.join("greetings.txt")).unwrap();
        assert_eq!(content, "Hello from tar.gz!");
    }

    #[test]
    fn test_extract_tar_xz_simple() {
        let temp_dir = TempDir::new().unwrap();
        let archive_path = temp_dir.path().join("test.tar.xz");
        let extract_dir = temp_dir.path().join("extracted");

        // Create a simple tar.xz file
        {
            let file = File::create(&archive_path).unwrap();
            let encoder = xz2::write::XzEncoder::new(file, 6);
            let mut builder = tar::Builder::new(encoder);

            let data = b"Hello from tar.xz!";
            let mut header = tar::Header::new_gnu();
            header.set_path("greetings.txt").unwrap();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();

            builder.append(&header, &data[..]).unwrap();
            builder.finish().unwrap();
        }

        extract_archive(&archive_path, &extract_dir, ArchiveFormat::TarXz).unwrap();

        assert!(extract_dir.join("greetings.txt").exists());
        let content = fs::read_to_string(extract_dir.join("greetings.txt")).unwrap();
        assert_eq!(content, "Hello from tar.xz!");
    }

    #[cfg(unix)]
    #[test]
    fn test_make_executable() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("script.sh");

        {
            let mut file = File::create(&file_path).unwrap();
            file.write_all(b"#!/bin/bash\necho hello").unwrap();

            let permissions = fs::Permissions::from_mode(0o644);
            fs::set_permissions(&file_path, permissions).unwrap();
        }

        let metadata = fs::metadata(&file_path).unwrap();
        assert_eq!(metadata.permissions().mode() & 0o111, 0);

        make_executable(&file_path).unwrap();

        let metadata = fs::metadata(&file_path).unwrap();
        assert_ne!(metadata.permissions().mode() & 0o111, 0);
    }

    #[test]
    fn test_tar_symlink_escape_blocked() {
        let temp_dir = TempDir::new().unwrap();
        let archive_path = temp_dir.path().join("malicious.tar.gz");
        let extract_dir = temp_dir.path().join("extracted");
        let escape_target = temp_dir.path().join("escaped_file.txt");

        {
            let file = File::create(&archive_path).unwrap();
            let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
            let mut builder = tar::Builder::new(encoder);

            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_path("escape_link").unwrap();
            header.set_size(0);
            header.set_mode(0o777);
            header.set_cksum();

            builder
                .append_link(&mut header, "escape_link", "../escaped_file.txt")
                .unwrap();

            let data = b"This should NOT appear outside extraction dir!";
            let mut file_header = tar::Header::new_gnu();
            file_header.set_path("escape_link").unwrap();
            file_header.set_size(data.len() as u64);
            file_header.set_mode(0o644);
            file_header.set_cksum();

            builder.append(&file_header, &data[..]).unwrap();
            builder.finish().unwrap();
        }

        extract_archive(&archive_path, &extract_dir, ArchiveFormat::TarGz).unwrap();

        assert!(
            !escape_target.exists(),
            "Symlink escape attack succeeded - file was written outside extraction dir!"
        );
        assert!(extract_dir.exists());
    }
}
