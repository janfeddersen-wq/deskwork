use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use glob::glob;
use thiserror::Error;

use crate::plugins::bundled::load_all_bundled_plugins;
use crate::plugins::types::{
    CommandFile, McpServersFile, Plugin, PluginManifest, PluginStatus, SkillFile,
};

#[derive(Debug, Error)]
pub enum PluginLoadError {
    #[error("Plugin manifest not found at {0}")]
    ManifestNotFound(PathBuf),

    #[error("Failed to read plugin manifest at {path}: {source}")]
    ManifestRead { path: PathBuf, source: io::Error },

    #[error("Invalid plugin manifest at {path}: {source}")]
    ManifestParse {
        path: PathBuf,
        source: serde_json::Error,
    },

    #[error("Failed to read plugins directory at {path}: {source}")]
    ReadDir { path: PathBuf, source: io::Error },
}

pub fn discover_plugins(plugins_dir: &Path) -> Vec<Result<Plugin, PluginLoadError>> {
    let Ok(entries) = fs::read_dir(plugins_dir) else {
        return Vec::new();
    };

    entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| path.join(".claude-plugin").join("plugin.json").exists())
        .map(|plugin_path| load_plugin(&plugin_path))
        .collect()
}

pub fn load_plugin(plugin_path: &Path) -> Result<Plugin, PluginLoadError> {
    let manifest_path = plugin_path.join(".claude-plugin").join("plugin.json");
    if !manifest_path.exists() {
        return Err(PluginLoadError::ManifestNotFound(manifest_path));
    }

    let manifest_raw =
        fs::read_to_string(&manifest_path).map_err(|source| PluginLoadError::ManifestRead {
            path: manifest_path.clone(),
            source,
        })?;

    let manifest: PluginManifest =
        serde_json::from_str(&manifest_raw).map_err(|source| PluginLoadError::ManifestParse {
            path: manifest_path,
            source,
        })?;

    let plugin_id = normalize_plugin_id(&manifest.name);
    let mut errors = Vec::new();

    let skills = load_skills(plugin_path, &mut errors);
    let commands = load_commands(plugin_path, &plugin_id, &mut errors);
    let mcp_servers = load_mcp_servers(plugin_path, &mut errors);
    let local_config = load_local_config(plugin_path, &plugin_id, &mut errors);

    let status = if errors.is_empty() {
        PluginStatus::Inactive
    } else {
        PluginStatus::Error
    };

    Ok(Plugin {
        id: plugin_id,
        name: manifest.name,
        version: manifest.version,
        description: manifest.description,
        path: plugin_path.to_path_buf(),
        enabled: false,
        skills,
        commands,
        mcp_servers,
        local_config,
        status,
        errors,
    })
}

pub fn load_bundled_plugins() -> Vec<Plugin> {
    load_all_bundled_plugins()
}

fn load_skills(plugin_path: &Path, errors: &mut Vec<String>) -> Vec<SkillFile> {
    let mut skills = Vec::new();
    let pattern = plugin_path.join("skills").join("**").join("SKILL.md");

    let entries = match glob(&pattern.to_string_lossy()) {
        Ok(entries) => entries,
        Err(err) => {
            errors.push(format!(
                "Invalid skills glob pattern '{}': {err}",
                pattern.display()
            ));
            return skills;
        }
    };

    for entry in entries {
        let path = match entry {
            Ok(path) => path,
            Err(err) => {
                errors.push(format!("Failed to resolve skill path: {err}"));
                continue;
            }
        };

        let markdown = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(err) => {
                errors.push(format!(
                    "Failed reading skill file '{}': {err}",
                    path.display()
                ));
                continue;
            }
        };

        let skill = SkillFile::from_markdown(&path, &markdown);
        if skill.description.trim().is_empty() {
            errors.push(format!(
                "Skill '{}' is missing frontmatter description",
                path.display()
            ));
        }
        skills.push(skill);
    }

    skills
}

fn load_commands(
    plugin_path: &Path,
    plugin_id: &str,
    errors: &mut Vec<String>,
) -> Vec<CommandFile> {
    let mut commands = Vec::new();
    let pattern = plugin_path.join("commands").join("*.md");

    let entries = match glob(&pattern.to_string_lossy()) {
        Ok(entries) => entries,
        Err(err) => {
            errors.push(format!(
                "Invalid commands glob pattern '{}': {err}",
                pattern.display()
            ));
            return commands;
        }
    };

    for entry in entries {
        let path = match entry {
            Ok(path) => path,
            Err(err) => {
                errors.push(format!("Failed to resolve command path: {err}"));
                continue;
            }
        };

        let markdown = match fs::read_to_string(&path) {
            Ok(content) => content,
            Err(err) => {
                errors.push(format!(
                    "Failed reading command file '{}': {err}",
                    path.display()
                ));
                continue;
            }
        };

        let command = CommandFile::from_markdown(&path, plugin_id.to_string(), &markdown);
        if command.description.trim().is_empty() {
            errors.push(format!(
                "Command '{}' is missing frontmatter description",
                path.display()
            ));
        }
        commands.push(command);
    }

    commands
}

fn load_mcp_servers(
    plugin_path: &Path,
    errors: &mut Vec<String>,
) -> HashMap<String, crate::plugins::types::McpServerEntry> {
    let path = plugin_path.join(".mcp.json");
    if !path.exists() {
        return HashMap::new();
    }

    let raw = match fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(err) => {
            errors.push(format!(
                "Failed reading MCP config '{}': {err}",
                path.display()
            ));
            return HashMap::new();
        }
    };

    match serde_json::from_str::<McpServersFile>(&raw) {
        Ok(file) => file.mcp_servers,
        Err(err) => {
            errors.push(format!(
                "Failed parsing MCP config '{}': {err}",
                path.display()
            ));
            HashMap::new()
        }
    }
}

fn load_local_config(
    plugin_path: &Path,
    plugin_id: &str,
    errors: &mut Vec<String>,
) -> Option<String> {
    let local_path = plugin_path.join(format!("{plugin_id}.local.md"));
    if !local_path.exists() {
        return None;
    }

    match fs::read_to_string(&local_path) {
        Ok(content) => Some(content),
        Err(err) => {
            errors.push(format!(
                "Failed reading local config '{}': {err}",
                local_path.display()
            ));
            None
        }
    }
}

fn normalize_plugin_id(name: &str) -> String {
    let mut id = String::with_capacity(name.len());

    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            id.push(ch.to_ascii_lowercase());
        } else if ch == '-' || ch == '_' || ch.is_ascii_whitespace() {
            id.push('-');
        }
    }

    let id = id.trim_matches('-').to_string();
    if id.is_empty() {
        "plugin".to_string()
    } else {
        id
    }
}
