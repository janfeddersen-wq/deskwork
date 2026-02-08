use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

// Re-export shared types from canonical location
pub use crate::skills::types::{
    parse_frontmatter, CommandFile, CommandFrontmatter, McpServerEntry, McpServersFile, SkillFile,
    SkillFrontmatter,
};

// Plugin-specific types (will be removed when plugins/ module is deleted)

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
