//! Unified diff parsing and application.
//!
//! Supports standard unified diff format:
//! ```text
//! --- a/file.txt
//! +++ b/file.txt
//! @@ -1,3 +1,4 @@
//!  context line
//! -removed line
//! +added line
//!  more context
//! ```

use std::str::Lines;
use thiserror::Error;

/// Diff parsing/application errors.
#[derive(Debug, Error)]
pub enum DiffError {
    #[error("Invalid diff format: {0}")]
    InvalidFormat(String),

    #[error("Hunk header parse error: {0}")]
    HunkParseError(String),

    #[error("Context mismatch at line {line}: expected '{expected}', got '{actual}'")]
    ContextMismatch {
        line: usize,
        expected: String,
        actual: String,
    },

    #[error("Patch application failed: {0}")]
    PatchFailed(String),
}

/// A single hunk in a diff.
#[derive(Debug, Clone)]
pub struct Hunk {
    pub old_start: usize,
    pub old_count: usize,
    pub new_start: usize,
    pub new_count: usize,
    pub lines: Vec<DiffLine>,
}

/// A line in a diff hunk.
#[derive(Debug, Clone)]
pub enum DiffLine {
    Context(String),
    Add(String),
    Remove(String),
}

/// A parsed unified diff.
#[derive(Debug, Clone)]
pub struct UnifiedDiff {
    pub old_path: Option<String>,
    pub new_path: Option<String>,
    pub is_new_file: bool,
    pub is_delete: bool,
    pub hunks: Vec<Hunk>,
}

impl UnifiedDiff {
    /// Parse a unified diff from text.
    pub fn parse(diff_text: &str) -> Result<Self, DiffError> {
        let mut lines = diff_text.lines().peekable();
        let mut old_path = None;
        let mut new_path = None;
        let mut is_new_file = false;
        let mut is_delete = false;
        let mut hunks = Vec::new();

        // Parse header lines
        while let Some(line) = lines.peek() {
            if line.starts_with("---") {
                let path = parse_file_path(line, "---");
                is_new_file = path == "/dev/null";
                old_path = if is_new_file { None } else { Some(path) };
                lines.next();
            } else if line.starts_with("+++") {
                let path = parse_file_path(line, "+++");
                is_delete = path == "/dev/null";
                new_path = if is_delete { None } else { Some(path) };
                lines.next();
            } else if line.starts_with("@@") {
                break;
            } else {
                lines.next();
            }
        }

        // Parse hunks
        while let Some(line) = lines.peek() {
            if line.starts_with("@@") {
                let hunk = parse_hunk(&mut lines)?;
                hunks.push(hunk);
            } else {
                lines.next();
            }
        }

        Ok(UnifiedDiff {
            old_path,
            new_path,
            is_new_file,
            is_delete,
            hunks,
        })
    }

    /// Apply this diff to the given content.
    pub fn apply(&self, original: &str) -> Result<String, DiffError> {
        if self.is_new_file {
            let mut result = String::new();
            for hunk in &self.hunks {
                for line in &hunk.lines {
                    if let DiffLine::Add(content) = line {
                        result.push_str(content);
                        result.push('\n');
                    }
                }
            }
            if result.ends_with('\n') && !result.ends_with("\n\n") {
                result.pop();
            }
            return Ok(result);
        }

        if self.is_delete {
            return Ok(String::new());
        }

        let mut lines: Vec<String> = original.lines().map(|s| s.to_string()).collect();

        for hunk in self.hunks.iter().rev() {
            lines = apply_hunk_to_lines(lines, hunk)?;
        }

        Ok(lines.join("\n"))
    }
}

fn parse_file_path(line: &str, prefix: &str) -> String {
    let path = line.strip_prefix(prefix).unwrap_or(line).trim();
    let path = if path.starts_with("a/") || path.starts_with("b/") {
        &path[2..]
    } else {
        path
    };
    let path = path.split('\t').next().unwrap_or(path);
    path.to_string()
}

fn parse_hunk(lines: &mut std::iter::Peekable<Lines>) -> Result<Hunk, DiffError> {
    let header = lines
        .next()
        .ok_or_else(|| DiffError::HunkParseError("Expected hunk header".to_string()))?;

    let (old_start, old_count, new_start, new_count) = parse_hunk_header(header)?;
    let mut hunk_lines = Vec::new();

    while let Some(line) = lines.peek() {
        if line.starts_with("@@") || line.starts_with("---") || line.starts_with("+++") {
            break;
        }

        let line = lines.next().unwrap();

        if line.is_empty() {
            hunk_lines.push(DiffLine::Context(String::new()));
        } else if let Some(content) = line.strip_prefix('+') {
            hunk_lines.push(DiffLine::Add(content.to_string()));
        } else if let Some(content) = line.strip_prefix('-') {
            hunk_lines.push(DiffLine::Remove(content.to_string()));
        } else if let Some(content) = line.strip_prefix(' ') {
            hunk_lines.push(DiffLine::Context(content.to_string()));
        } else if line.starts_with('\\') {
            continue; // "\ No newline at end of file"
        } else {
            hunk_lines.push(DiffLine::Context(line.to_string()));
        }
    }

    Ok(Hunk {
        old_start,
        old_count,
        new_start,
        new_count,
        lines: hunk_lines,
    })
}

fn parse_hunk_header(header: &str) -> Result<(usize, usize, usize, usize), DiffError> {
    let header = header
        .strip_prefix("@@")
        .and_then(|s| s.split("@@").next())
        .ok_or_else(|| DiffError::HunkParseError(format!("Invalid header: {}", header)))?;

    let parts: Vec<&str> = header.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(DiffError::HunkParseError(format!(
            "Invalid header: {}",
            header
        )));
    }

    let (old_start, old_count) = parse_range(parts[0].strip_prefix('-').unwrap_or(parts[0]))?;
    let (new_start, new_count) = parse_range(parts[1].strip_prefix('+').unwrap_or(parts[1]))?;

    Ok((old_start, old_count, new_start, new_count))
}

fn parse_range(range: &str) -> Result<(usize, usize), DiffError> {
    let parts: Vec<&str> = range.split(',').collect();
    let start = parts[0]
        .parse::<usize>()
        .map_err(|_| DiffError::HunkParseError(format!("Invalid range: {}", range)))?;
    let count = if parts.len() > 1 {
        parts[1]
            .parse::<usize>()
            .map_err(|_| DiffError::HunkParseError(format!("Invalid range: {}", range)))?
    } else {
        1
    };
    Ok((start, count))
}

fn apply_hunk_to_lines(lines: Vec<String>, hunk: &Hunk) -> Result<Vec<String>, DiffError> {
    let start_idx = if hunk.old_start == 0 {
        0
    } else {
        hunk.old_start - 1
    };
    let mut new_lines = Vec::new();

    // Lines before hunk
    new_lines.extend(lines.iter().take(start_idx).cloned());

    // Apply hunk
    for diff_line in &hunk.lines {
        match diff_line {
            DiffLine::Context(content) | DiffLine::Add(content) => {
                new_lines.push(content.clone());
            }
            DiffLine::Remove(_) => {}
        }
    }

    // Lines after hunk
    let skip_count = hunk
        .lines
        .iter()
        .filter(|l| matches!(l, DiffLine::Context(_) | DiffLine::Remove(_)))
        .count();

    new_lines.extend(lines.iter().skip(start_idx + skip_count).cloned());

    Ok(new_lines)
}

/// Apply a unified diff to file content.
pub fn apply_unified_diff(original: &str, diff_text: &str) -> Result<String, DiffError> {
    let diff = UnifiedDiff::parse(diff_text)?;
    diff.apply(original)
}

/// Check if text looks like a unified diff.
pub fn is_unified_diff(text: &str) -> bool {
    text.contains("@@") && (text.contains("---") || text.contains("+++"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_diff() {
        let diff = r#"--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,4 @@
 line 1
-line 2
+line 2 modified
+line 2.5 added
 line 3
"#;
        let parsed = UnifiedDiff::parse(diff).unwrap();
        assert_eq!(parsed.old_path, Some("file.txt".to_string()));
        assert_eq!(parsed.hunks.len(), 1);
    }

    #[test]
    fn test_apply_simple_diff() {
        let original = "line 1\nline 2\nline 3";
        let diff = r#"--- a/file.txt
+++ b/file.txt
@@ -1,3 +1,4 @@
 line 1
-line 2
+line 2 modified
+line 2.5 added
 line 3
"#;
        let result = apply_unified_diff(original, diff).unwrap();
        assert_eq!(result, "line 1\nline 2 modified\nline 2.5 added\nline 3");
    }

    #[test]
    fn test_new_file_diff() {
        let diff = r#"--- /dev/null
+++ b/new_file.txt
@@ -0,0 +1,3 @@
+line 1
+line 2
+line 3
"#;
        let parsed = UnifiedDiff::parse(diff).unwrap();
        assert!(parsed.is_new_file);

        let result = apply_unified_diff("", diff).unwrap();
        assert_eq!(result, "line 1\nline 2\nline 3");
    }

    #[test]
    fn test_delete_file_diff() {
        let diff = r#"--- a/old_file.txt
+++ /dev/null
@@ -1,3 +0,0 @@
-line 1
-line 2
-line 3
"#;
        let parsed = UnifiedDiff::parse(diff).unwrap();
        assert!(parsed.is_delete);

        let result = apply_unified_diff("line 1\nline 2\nline 3", diff).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_is_unified_diff() {
        assert!(is_unified_diff("--- a/file\n+++ b/file\n@@"));
        assert!(!is_unified_diff("just some text"));
    }
}
