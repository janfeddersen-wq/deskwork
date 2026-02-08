use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginAuthor {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: PluginAuthor,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpServersFile {
    #[serde(rename = "mcpServers")]
    pub mcp_servers: HashMap<String, McpServerEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpServerEntry {
    #[serde(rename = "type")]
    pub r#type: String,
    pub url: Option<String>,
    pub command: Option<String>,
    pub args: Option<Vec<String>>,
    pub env: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillFrontmatter {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandFrontmatter {
    pub description: String,
    #[serde(rename = "argument-hint")]
    pub argument_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillFile {
    pub name: String,
    pub description: String,
    pub content: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandFile {
    pub name: String,
    pub description: String,
    pub argument_hint: Option<String>,
    pub content: String,
    pub path: PathBuf,
    pub plugin_id: String,
    pub slash_command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum PluginStatus {
    Active,
    Inactive,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Plugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub path: PathBuf,
    pub enabled: bool,
    pub skills: Vec<SkillFile>,
    pub commands: Vec<CommandFile>,
    pub mcp_servers: HashMap<String, McpServerEntry>,
    pub local_config: Option<String>,
    pub status: PluginStatus,
    pub errors: Vec<String>,
}

/// Parse markdown frontmatter in the form:
/// ---
/// key: value
/// key2: value
/// ---
/// body...
///
/// Returns `(frontmatter_map, markdown_body)`.
pub fn parse_frontmatter(content: &str) -> (HashMap<String, String>, &str) {
    let mut map = HashMap::new();

    if !content.starts_with("---") {
        return (map, content);
    }

    let mut lines = content.lines();
    if lines.next() != Some("---") {
        return (map, content);
    }

    let mut consumed = 4; // first `---\n`
    let mut found_end = false;

    for line in lines {
        consumed += line.len() + 1; // line + trailing newline

        if line.trim() == "---" {
            found_end = true;
            break;
        }

        let Some((raw_key, raw_value)) = line.split_once(':') else {
            continue;
        };

        let key = raw_key.trim().to_string();
        let mut value = raw_value.trim().to_string();

        if value.starts_with('"') && value.ends_with('"') && value.len() >= 2 {
            value = value[1..value.len() - 1].to_string();
        }

        map.insert(key, value);
    }

    if !found_end {
        return (HashMap::new(), content);
    }

    let body = content.get(consumed..).unwrap_or("");
    (map, body)
}

impl SkillFile {
    pub fn from_markdown(path: impl AsRef<Path>, markdown: &str) -> Self {
        let (frontmatter, body) = parse_frontmatter(markdown);

        Self {
            name: frontmatter
                .get("name")
                .cloned()
                .unwrap_or_else(|| "unknown-skill".to_string()),
            description: frontmatter.get("description").cloned().unwrap_or_default(),
            content: body.trim_start().to_string(),
            path: path.as_ref().to_path_buf(),
        }
    }
}

impl CommandFile {
    pub fn from_markdown(
        path: impl AsRef<Path>,
        plugin_id: impl Into<String>,
        markdown: &str,
    ) -> Self {
        let path_ref = path.as_ref();
        let (frontmatter, body) = parse_frontmatter(markdown);

        let stem = path_ref
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown-command")
            .to_string();

        let plugin_id = plugin_id.into();

        Self {
            name: stem.clone(),
            description: frontmatter.get("description").cloned().unwrap_or_default(),
            argument_hint: frontmatter.get("argument-hint").cloned(),
            content: body.trim_start().to_string(),
            path: path_ref.to_path_buf(),
            plugin_id: plugin_id.clone(),
            slash_command: format!("/{plugin_id}:{stem}"),
        }
    }
}
