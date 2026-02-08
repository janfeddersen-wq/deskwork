//! Skill discovery and metadata parsing.

use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use tracing::{debug, trace, warn};

use crate::external_tools::paths::get_skills_dir;

/// Metadata extracted from a skill's SKILL.md file.
#[derive(Debug, Clone, PartialEq)]
pub struct SkillMetadata {
    /// The skill name (from frontmatter `name` field).
    pub name: String,
    /// Description of what the skill does.
    pub description: String,
    /// License information.
    pub license: String,
    /// Path to the skill directory.
    pub path: PathBuf,
    /// Full path to the SKILL.md file.
    pub skill_md_path: PathBuf,
}

/// Parses YAML frontmatter from a SKILL.md file content.
///
/// Returns `Some(SkillMetadata)` if frontmatter contains name, description, and license.
pub fn parse_skill_frontmatter(content: &str) -> Option<SkillMetadata> {
    let content = content.trim_start();
    if !content.starts_with("---") {
        trace!("No frontmatter marker found");
        return None;
    }

    let after_first_marker = &content[3..].trim_start();
    let end_marker_pos = after_first_marker.find("---")?;
    let frontmatter = &after_first_marker[..end_marker_pos];

    let mut name: Option<String> = None;
    let mut description: Option<String> = None;
    let mut license: Option<String> = None;

    for line in frontmatter.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        if let Some(colon_pos) = line.find(':') {
            let key = line[..colon_pos].trim().to_lowercase();
            let value = line[colon_pos + 1..].trim();
            let value = strip_quotes(value);

            match key.as_str() {
                "name" => name = Some(value.to_string()),
                "description" => description = Some(value.to_string()),
                "license" => license = Some(value.to_string()),
                _ => trace!("Ignoring unknown frontmatter key: {}", key),
            }
        }
    }

    match (name, description, license) {
        (Some(name), Some(description), Some(license)) => Some(SkillMetadata {
            name,
            description,
            license,
            path: PathBuf::new(),
            skill_md_path: PathBuf::new(),
        }),
        _ => {
            trace!("Missing required frontmatter fields");
            None
        }
    }
}

/// Strips surrounding quotes from a string value.
fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    if ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
        && s.len() >= 2
    {
        return &s[1..s.len() - 1];
    }
    s
}

/// Discovers all skills in the skills directory.
pub fn discover_skills() -> Result<Vec<SkillMetadata>> {
    let skills_dir = get_skills_dir();

    if !skills_dir.exists() {
        debug!("Skills directory does not exist: {}", skills_dir.display());
        return Ok(Vec::new());
    }

    let mut skills = Vec::new();

    let entries = fs::read_dir(&skills_dir)
        .with_context(|| format!("Failed to read skills directory: {}", skills_dir.display()))?;

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                warn!("Failed to read directory entry: {}", e);
                continue;
            }
        };

        let path = entry.path();

        if !path.is_dir() {
            trace!("Skipping non-directory: {}", path.display());
            continue;
        }

        let skill_md_path = path.join("SKILL.md");
        if !skill_md_path.exists() {
            trace!("No SKILL.md found in: {}", path.display());
            continue;
        }

        let content = match fs::read_to_string(&skill_md_path) {
            Ok(c) => c,
            Err(e) => {
                warn!(
                    "Failed to read SKILL.md at {}: {}",
                    skill_md_path.display(),
                    e
                );
                continue;
            }
        };

        match parse_skill_frontmatter(&content) {
            Some(mut metadata) => {
                metadata.path = path;
                metadata.skill_md_path = skill_md_path;
                debug!(
                    "Discovered skill: {} at {}",
                    metadata.name,
                    metadata.path.display()
                );
                skills.push(metadata);
            }
            None => {
                warn!(
                    "Failed to parse frontmatter in SKILL.md: {}",
                    skill_md_path.display()
                );
            }
        }
    }

    debug!("Discovered {} skills", skills.len());
    Ok(skills)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_skill_frontmatter_valid() {
        let content = r#"---
name: docx
description: "Comprehensive document creation, editing, and analysis"
license: Proprietary. LICENSE.txt has complete terms
---

# DOCX Skill
"#;

        let metadata = parse_skill_frontmatter(content);
        assert!(metadata.is_some());

        let metadata = metadata.unwrap();
        assert_eq!(metadata.name, "docx");
        assert_eq!(
            metadata.description,
            "Comprehensive document creation, editing, and analysis"
        );
        assert_eq!(
            metadata.license,
            "Proprietary. LICENSE.txt has complete terms"
        );
    }

    #[test]
    fn test_parse_skill_frontmatter_with_single_quotes() {
        let content = r#"---
name: 'my-skill'
description: 'A cool skill'
license: 'MIT'
---
"#;

        let metadata = parse_skill_frontmatter(content);
        assert!(metadata.is_some());

        let metadata = metadata.unwrap();
        assert_eq!(metadata.name, "my-skill");
        assert_eq!(metadata.description, "A cool skill");
        assert_eq!(metadata.license, "MIT");
    }

    #[test]
    fn test_parse_skill_frontmatter_missing_field() {
        let content = r#"---
name: docx
description: "A skill"
---
"#;

        let metadata = parse_skill_frontmatter(content);
        assert!(metadata.is_none(), "Should fail when license is missing");
    }

    #[test]
    fn test_parse_skill_frontmatter_no_markers() {
        let content = "# Just a markdown file\n\nNo frontmatter here.";

        let metadata = parse_skill_frontmatter(content);
        assert!(metadata.is_none());
    }

    #[test]
    fn test_parse_skill_frontmatter_case_insensitive_keys() {
        let content = r#"---
Name: my-skill
DESCRIPTION: A skill
License: MIT
---
"#;

        let metadata = parse_skill_frontmatter(content);
        assert!(metadata.is_some());
        assert_eq!(metadata.unwrap().name, "my-skill");
    }

    #[test]
    fn test_strip_quotes() {
        assert_eq!(strip_quotes("\"hello\""), "hello");
        assert_eq!(strip_quotes("'hello'"), "hello");
        assert_eq!(strip_quotes("hello"), "hello");
        assert_eq!(strip_quotes("  \"spaced\"  "), "spaced");
    }

    #[test]
    fn test_discover_skills_returns_ok() {
        let result = discover_skills();
        assert!(result.is_ok(), "discover_skills should not error");
    }
}
