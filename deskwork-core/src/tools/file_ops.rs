//! Core file operations.
//!
//! Provides low-level file system operations with safety limits.

use super::common::should_ignore;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use thiserror::Error;

// =============================================================================
// Constants
// =============================================================================

/// Maximum tokens allowed in a single file read to protect context window.
const READ_FILE_MAX_TOKENS: usize = 10_000;

/// Approximate characters per token (conservative estimate).
const CHARS_PER_TOKEN: usize = 4;

/// Default max entries for list_files.
const LIST_FILES_DEFAULT_MAX_ENTRIES: usize = 2_000;

/// Hard cap on max entries for list_files.
const LIST_FILES_HARD_MAX_ENTRIES: usize = 10_000;

/// Default max depth for recursive listing.
const LIST_FILES_DEFAULT_MAX_DEPTH: usize = 10;

/// Hard cap on max depth.
const LIST_FILES_HARD_MAX_DEPTH: usize = 50;

// =============================================================================
// Error Types
// =============================================================================

/// Errors that can occur during file operations.
#[derive(Debug, Error)]
pub enum FileError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Path not found: {0}")]
    NotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("File too large: {0} bytes (max: {1})")]
    TooLarge(u64, u64),

    #[error(
        "File too large: ~{estimated_tokens} tokens ({total_lines} lines). \
         Read in chunks using start_line and num_lines parameters. \
         Suggested: start_line=1, num_lines={suggested_chunk_size}"
    )]
    TokenLimitExceeded {
        estimated_tokens: usize,
        total_lines: usize,
        suggested_chunk_size: usize,
    },
}

// =============================================================================
// File Listing
// =============================================================================

/// File entry for directory listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub depth: usize,
}

/// Result of listing files.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListFilesResult {
    pub entries: Vec<FileEntry>,
    pub total_files: usize,
    pub total_dirs: usize,
    pub total_size: u64,
    pub truncated: bool,
    pub max_entries: usize,
}

/// Context for recursive file listing.
struct ListFilesContext<'a> {
    base: &'a Path,
    entries: &'a mut Vec<FileEntry>,
    recursive: bool,
    max_depth: usize,
    max_entries: usize,
    truncated: &'a mut bool,
}

/// Check if a directory is likely a home directory.
fn is_home_directory(path: &Path) -> bool {
    let Some(home_path) = dirs::home_dir() else {
        return false;
    };

    if path == home_path {
        return true;
    }

    // Check common home subdirectories
    const COMMON_SUBDIRS: &[&str] = &[
        "Documents",
        "Desktop",
        "Downloads",
        "Pictures",
        "Music",
        "Videos",
    ];

    if let Some(parent) = path.parent() {
        if parent == home_path {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                return COMMON_SUBDIRS.contains(&name);
            }
        }
    }

    false
}

/// Check if a directory looks like a project directory.
fn is_project_directory(path: &Path) -> bool {
    const INDICATORS: &[&str] = &[
        "package.json",
        "pyproject.toml",
        "Cargo.toml",
        "pom.xml",
        "build.gradle",
        "CMakeLists.txt",
        ".git",
        "requirements.txt",
        "composer.json",
        "Gemfile",
        "go.mod",
        "Makefile",
        "setup.py",
    ];

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if INDICATORS.contains(&name) {
                    return true;
                }
            }
        }
    }

    false
}

/// List files in a directory.
///
/// # Arguments
/// * `directory` - Path to the directory to list
/// * `recursive` - Whether to list recursively (auto-disabled for home dirs)
/// * `max_depth` - Maximum depth for recursive listing (default: 10, cap: 50)
/// * `max_entries` - Maximum entries to return (default: 2000, cap: 10000)
pub fn list_files(
    directory: &str,
    recursive: bool,
    max_depth: Option<usize>,
    max_entries: Option<usize>,
) -> Result<ListFilesResult, FileError> {
    let path = Path::new(directory);

    if !path.exists() {
        return Err(FileError::NotFound(directory.to_string()));
    }

    // Auto-disable recursion for home directories (unless it's a project)
    let effective_recursive = recursive && (!is_home_directory(path) || is_project_directory(path));

    let max_entries = max_entries
        .unwrap_or(LIST_FILES_DEFAULT_MAX_ENTRIES)
        .clamp(1, LIST_FILES_HARD_MAX_ENTRIES);

    let max_depth = max_depth
        .unwrap_or(LIST_FILES_DEFAULT_MAX_DEPTH)
        .min(LIST_FILES_HARD_MAX_DEPTH);

    let mut entries = Vec::new();
    let mut truncated = false;

    let mut ctx = ListFilesContext {
        base: path,
        entries: &mut entries,
        recursive: effective_recursive,
        max_depth,
        max_entries,
        truncated: &mut truncated,
    };

    list_files_recursive(&mut ctx, path, 0)?;

    // Calculate totals
    let (total_files, total_dirs, total_size) =
        entries
            .iter()
            .fold((0, 0, 0u64), |(files, dirs, size), entry| {
                if entry.is_dir {
                    (files, dirs + 1, size)
                } else {
                    (files + 1, dirs, size + entry.size)
                }
            });

    Ok(ListFilesResult {
        entries,
        total_files,
        total_dirs,
        total_size,
        truncated,
        max_entries,
    })
}

fn list_files_recursive(
    ctx: &mut ListFilesContext,
    dir: &Path,
    depth: usize,
) -> Result<(), FileError> {
    if depth > ctx.max_depth || ctx.entries.len() >= ctx.max_entries {
        if ctx.entries.len() >= ctx.max_entries {
            *ctx.truncated = true;
        }
        return Ok(());
    }

    let read_dir = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if depth == 0 => return Err(e.into()),
        Err(_) => return Ok(()), // Skip unreadable subdirectories
    };

    let mut dir_entries: Vec<_> = read_dir.filter_map(|e| e.ok()).collect();
    dir_entries.sort_by_key(|a| a.file_name());

    for entry in dir_entries {
        if ctx.entries.len() >= ctx.max_entries {
            *ctx.truncated = true;
            break;
        }

        let path = entry.path();
        let relative = path.strip_prefix(ctx.base).unwrap_or(&path);
        let relative_str = relative.to_string_lossy().to_string();

        if should_ignore(&relative_str) {
            continue;
        }

        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        let is_dir = file_type.is_dir();
        let name = entry.file_name().to_string_lossy().to_string();

        ctx.entries.push(FileEntry {
            path: relative_str,
            name,
            is_dir,
            size: if is_dir { 0 } else { metadata.len() },
            depth,
        });

        if is_dir && ctx.recursive {
            list_files_recursive(ctx, &path, depth + 1)?;
        }
    }

    Ok(())
}

// =============================================================================
// File Reading
// =============================================================================

/// Result of reading a file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadFileResult {
    pub content: String,
    pub path: String,
    pub size: u64,
    pub lines: usize,
    pub estimated_tokens: usize,
}

/// Read file contents.
///
/// # Arguments
/// * `path` - Path to the file
/// * `start_line` - Starting line (1-based, optional)
/// * `num_lines` - Number of lines to read (optional)
/// * `max_size` - Maximum file size in bytes (default: 10MB)
pub fn read_file(
    path: &str,
    start_line: Option<usize>,
    num_lines: Option<usize>,
    max_size: Option<u64>,
) -> Result<ReadFileResult, FileError> {
    let file_path = Path::new(path);

    if !file_path.exists() {
        return Err(FileError::NotFound(path.to_string()));
    }

    let metadata = fs::metadata(file_path)?;
    let max = max_size.unwrap_or(10 * 1024 * 1024); // 10MB default

    if metadata.len() > max {
        return Err(FileError::TooLarge(metadata.len(), max));
    }

    let content = fs::read_to_string(file_path)?;
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    // Token-based protection (only for full file reads)
    if start_line.is_none() {
        let estimated_tokens = content.len() / CHARS_PER_TOKEN;
        if estimated_tokens > READ_FILE_MAX_TOKENS {
            let suggested_chunk = total_lines.div_ceil(4).min(500);
            return Err(FileError::TokenLimitExceeded {
                estimated_tokens,
                total_lines,
                suggested_chunk_size: suggested_chunk,
            });
        }
    }

    let content = if let Some(start) = start_line {
        let start_idx = start.saturating_sub(1); // 1-based to 0-based
        let end_idx = num_lines
            .map(|n| (start_idx + n).min(total_lines))
            .unwrap_or(total_lines);

        lines
            .get(start_idx..end_idx)
            .map(|slice| slice.join("\n"))
            .unwrap_or_default()
    } else {
        content
    };

    let estimated_tokens = content.len() / CHARS_PER_TOKEN;

    Ok(ReadFileResult {
        content,
        path: path.to_string(),
        size: metadata.len(),
        lines: total_lines,
        estimated_tokens,
    })
}

// =============================================================================
// File Writing
// =============================================================================

/// Write content to a file.
///
/// # Arguments
/// * `path` - Path to the file
/// * `content` - Content to write
/// * `create_dirs` - Whether to create parent directories
pub fn write_file(path: &str, content: &str, create_dirs: bool) -> Result<(), FileError> {
    let file_path = Path::new(path);

    if create_dirs {
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)?;
        }
    }

    fs::write(file_path, content)?;
    Ok(())
}

/// Delete a file.
pub fn delete_file(path: &str) -> Result<(), FileError> {
    let file_path = Path::new(path);

    if !file_path.exists() {
        return Err(FileError::NotFound(path.to_string()));
    }

    fs::remove_file(file_path)?;
    Ok(())
}

// =============================================================================
// Grep / Search
// =============================================================================

/// A single grep match.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepMatch {
    pub path: String,
    pub line_number: usize,
    pub content: String,
}

/// Result of a grep search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GrepResult {
    pub matches: Vec<GrepMatch>,
    pub total_matches: usize,
}

/// Safety caps for grep.
const GREP_MAX_MATCHES: usize = 100;
const GREP_MAX_LINE_LENGTH: usize = 512;
const GREP_MAX_FILE_SIZE: u64 = 5 * 1024 * 1024; // 5MB
const GREP_MAX_DEPTH: usize = 10;

/// Search for a pattern in files.
///
/// Uses regex for pattern matching. Falls back to literal search if regex is invalid.
pub fn grep(
    pattern: &str,
    directory: &str,
    max_results: Option<usize>,
) -> Result<GrepResult, FileError> {
    use regex::Regex;
    #[allow(unused_imports)]
    use std::io::{BufRead, BufReader};

    let max_matches = max_results
        .unwrap_or(GREP_MAX_MATCHES)
        .min(GREP_MAX_MATCHES);

    if pattern.is_empty() {
        return Err(FileError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "pattern must not be empty",
        )));
    }

    let path = Path::new(directory);
    if !path.exists() {
        return Err(FileError::NotFound(directory.to_string()));
    }
    if !path.is_dir() {
        return Err(FileError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a directory",
        )));
    }

    // Parse flags from pattern (e.g., "--ignore-case pattern")
    let (pattern, case_insensitive) = if let Some(rest) = pattern.strip_prefix("--ignore-case ") {
        (rest.trim(), true)
    } else if let Some(rest) = pattern.strip_prefix("-i ") {
        (rest.trim(), true)
    } else {
        (pattern, false)
    };

    // Try regex, fall back to literal
    let regex_pattern = if case_insensitive {
        format!("(?i){}", pattern)
    } else {
        pattern.to_string()
    };

    let re = Regex::new(&regex_pattern)
        .or_else(|_| {
            let escaped = regex::escape(pattern);
            if case_insensitive {
                Regex::new(&format!("(?i){}", escaped))
            } else {
                Regex::new(&escaped)
            }
        })
        .map_err(|e| {
            FileError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("invalid pattern: {}", e),
            ))
        })?;

    let mut matches = Vec::new();
    grep_recursive(&re, path, path, &mut matches, max_matches, 0)?;

    Ok(GrepResult {
        total_matches: matches.len(),
        matches,
    })
}

fn grep_recursive(
    re: &regex::Regex,
    base: &Path,
    dir: &Path,
    matches: &mut Vec<GrepMatch>,
    max_matches: usize,
    depth: usize,
) -> Result<(), FileError> {
    #[allow(unused_imports)]
    use std::io::{BufRead, BufReader};

    if depth > GREP_MAX_DEPTH || matches.len() >= max_matches {
        return Ok(());
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(()), // Skip unreadable dirs
    };

    for entry in entries.flatten() {
        if matches.len() >= max_matches {
            break;
        }

        let path = entry.path();
        let relative = path.strip_prefix(base).unwrap_or(&path);
        let relative_str = relative.to_string_lossy().to_string();

        if should_ignore(&relative_str) {
            continue;
        }

        if path.is_dir() {
            grep_recursive(re, base, &path, matches, max_matches, depth + 1)?;
        } else if path.is_file() {
            // Check file size
            if let Ok(meta) = path.metadata() {
                if meta.len() > GREP_MAX_FILE_SIZE {
                    continue;
                }
            }

            // Check if text file
            if !super::common::is_text_file(&relative_str) {
                continue;
            }

            // Search file
            let file = match fs::File::open(&path) {
                Ok(f) => f,
                Err(_) => continue,
            };

            let reader = BufReader::new(file);
            for (line_num, line_result) in reader.lines().enumerate() {
                if matches.len() >= max_matches {
                    break;
                }

                let line = match line_result {
                    Ok(l) => l,
                    Err(_) => continue,
                };

                if re.is_match(&line) {
                    let content = if line.len() > GREP_MAX_LINE_LENGTH {
                        format!("{}...", &line[..GREP_MAX_LINE_LENGTH])
                    } else {
                        line
                    };

                    matches.push(GrepMatch {
                        path: relative_str.clone(),
                        line_number: line_num + 1,
                        content,
                    });
                }
            }
        }
    }

    Ok(())
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // -------------------------------------------------------------------------
    // list_files Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_list_files_basic() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.txt"), "a").unwrap();
        fs::write(dir.path().join("b.txt"), "b").unwrap();

        let result = list_files(dir.path().to_str().unwrap(), false, None, None).unwrap();

        assert_eq!(result.total_files, 2);
        assert!(!result.truncated);
    }

    #[test]
    fn test_list_files_recursive() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("root.txt"), "root").unwrap();
        fs::write(dir.path().join("sub").join("child.txt"), "child").unwrap();

        let result = list_files(dir.path().to_str().unwrap(), true, None, None).unwrap();

        assert!(result.total_files >= 2);
        assert!(result.total_dirs >= 1);
    }

    #[test]
    fn test_list_files_respects_max_entries() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.txt"), "a").unwrap();
        fs::write(dir.path().join("b.txt"), "b").unwrap();
        fs::write(dir.path().join("c.txt"), "c").unwrap();

        let result = list_files(dir.path().to_str().unwrap(), false, None, Some(2)).unwrap();

        assert_eq!(result.entries.len(), 2);
        assert!(result.truncated);
    }

    #[test]
    fn test_list_files_tracks_depth() {
        let dir = TempDir::new().unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("root.txt"), "root").unwrap();
        fs::write(dir.path().join("sub").join("child.txt"), "child").unwrap();

        let result = list_files(dir.path().to_str().unwrap(), true, None, None).unwrap();

        let root_entry = result
            .entries
            .iter()
            .find(|e| e.name == "root.txt")
            .unwrap();
        let child_entry = result
            .entries
            .iter()
            .find(|e| e.name == "child.txt")
            .unwrap();

        assert_eq!(root_entry.depth, 0);
        assert_eq!(child_entry.depth, 1);
    }

    #[test]
    fn test_list_files_not_found() {
        let result = list_files("/nonexistent/path", false, None, None);
        assert!(matches!(result, Err(FileError::NotFound(_))));
    }

    // -------------------------------------------------------------------------
    // read_file Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_read_file_basic() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "Hello, World!").unwrap();

        let result = read_file(file_path.to_str().unwrap(), None, None, None).unwrap();

        assert_eq!(result.content, "Hello, World!");
        assert_eq!(result.lines, 1);
    }

    #[test]
    fn test_read_file_with_line_range() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "Line 1\nLine 2\nLine 3\nLine 4\nLine 5").unwrap();

        let result = read_file(file_path.to_str().unwrap(), Some(2), Some(2), None).unwrap();

        assert_eq!(result.content, "Line 2\nLine 3");
        assert_eq!(result.lines, 5); // Total lines in file
    }

    #[test]
    fn test_read_file_out_of_bounds_start_line() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "Line 1\nLine 2\nLine 3").unwrap();

        let result = read_file(file_path.to_str().unwrap(), Some(100), Some(10), None).unwrap();

        assert!(result.content.is_empty());
        assert_eq!(result.lines, 3);
    }

    #[test]
    fn test_read_file_not_found() {
        let result = read_file("/nonexistent/file.txt", None, None, None);
        assert!(matches!(result, Err(FileError::NotFound(_))));
    }

    #[test]
    fn test_read_file_token_limit() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("large.txt");
        let large_content = "x".repeat(50_000); // ~12,500 tokens
        fs::write(&file_path, &large_content).unwrap();

        let result = read_file(file_path.to_str().unwrap(), None, None, None);

        assert!(matches!(result, Err(FileError::TokenLimitExceeded { .. })));
    }

    #[test]
    fn test_read_file_token_estimate() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "a".repeat(100)).unwrap();

        let result = read_file(file_path.to_str().unwrap(), None, None, None).unwrap();

        assert_eq!(result.estimated_tokens, 25); // 100 / 4
    }

    // -------------------------------------------------------------------------
    // write_file Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_write_file_basic() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");

        write_file(file_path.to_str().unwrap(), "Hello!", false).unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Hello!");
    }

    #[test]
    fn test_write_file_creates_dirs() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("sub").join("dir").join("test.txt");

        write_file(file_path.to_str().unwrap(), "Nested!", true).unwrap();

        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "Nested!");
    }

    // -------------------------------------------------------------------------
    // delete_file Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_delete_file_basic() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "Delete me").unwrap();

        delete_file(file_path.to_str().unwrap()).unwrap();

        assert!(!file_path.exists());
    }

    #[test]
    fn test_delete_file_not_found() {
        let result = delete_file("/nonexistent/file.txt");
        assert!(matches!(result, Err(FileError::NotFound(_))));
    }
}
