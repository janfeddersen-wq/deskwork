use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

// Include the build-generated bundled categories.
mod generated {
    include!(concat!(env!("OUT_DIR"), "/bundled_categories.rs"));
}

use generated::*;

// Reuse the existing types for now. We'll move these later.
use crate::skills::types::{CommandFile, McpServerEntry, McpServersFile, SkillFile};

/// Status of a skill category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CategoryStatus {
    Active,
    Inactive,
    Error,
}

/// A skill category (e.g., "legal", "finance", "sales")
///
/// Replaces the old `Plugin` type. Each category bundles:
/// - Skills (SKILL.md markdown files injected into system prompt)
/// - Commands (slash command templates)
/// - Connector configs (MCP servers)
#[derive(Debug, Clone)]
pub struct SkillCategory {
    pub id: String,
    pub name: String,
    pub description: String,
    pub enabled: bool,
    pub skills: Vec<SkillFile>,
    pub commands: Vec<CommandFile>,
    pub mcp_servers: HashMap<String, McpServerEntry>,
    pub connectors_doc: String,
    pub status: CategoryStatus,
    pub errors: Vec<String>,
}

/// Registry of all available skill categories.
#[derive(Debug, Default, Clone)]
pub struct SkillCategoryRegistry {
    categories: HashMap<String, SkillCategory>,
}

impl SkillCategoryRegistry {
    /// Load all bundled categories, marking any IDs in `enabled_ids` as enabled.
    pub fn load(enabled_ids: &[String]) -> Self {
        let mut registry = Self::default();

        for category in load_bundled_categories() {
            registry
                .categories
                .insert(category.id.clone(), category);
        }

        let enabled: HashSet<&str> = enabled_ids.iter().map(String::as_str).collect();

        for category in registry.categories.values_mut() {
            category.enabled = enabled.contains(category.id.as_str());
            category.status = compute_status(category.enabled, &category.errors);
        }

        registry
    }

    pub fn enable(&mut self, id: &str) {
        if let Some(category) = self.categories.get_mut(id) {
            category.enabled = true;
            category.status = compute_status(category.enabled, &category.errors);
        }
    }

    pub fn disable(&mut self, id: &str) {
        if let Some(category) = self.categories.get_mut(id) {
            category.enabled = false;
            category.status = compute_status(category.enabled, &category.errors);
        }
    }

    pub fn get_category(&self, id: &str) -> Option<&SkillCategory> {
        self.categories.get(id)
    }

    /// All skills from enabled + healthy categories.
    pub fn get_active_skills(&self) -> Vec<&SkillFile> {
        self.categories
            .values()
            .filter(|category| category.enabled && matches!(category.status, CategoryStatus::Active))
            .flat_map(|category| category.skills.iter())
            .collect()
    }

    /// All MCP configs from enabled + healthy categories.
    ///
    /// Key format: "{category_id}:{server_name}".
    pub fn get_active_mcp_configs(&self) -> HashMap<String, &McpServerEntry> {
        let mut configs = HashMap::new();

        for category in self
            .categories
            .values()
            .filter(|category| category.enabled && matches!(category.status, CategoryStatus::Active))
        {
            for (server_name, server) in &category.mcp_servers {
                let key = format!("{}:{}", category.id, server_name);
                configs.insert(key, server);
            }
        }

        configs
    }

    pub fn enabled_categories(&self) -> Vec<&SkillCategory> {
        self.categories
            .values()
            .filter(|category| category.enabled && matches!(category.status, CategoryStatus::Active))
            .collect()
    }

    /// All categories (enabled, disabled, error) sorted by name.
    pub fn all_categories(&self) -> Vec<&SkillCategory> {
        let mut categories = self.categories.values().collect::<Vec<_>>();
        categories.sort_by(|a, b| a.name.cmp(&b.name));
        categories
    }

    /// All slash commands from enabled + healthy categories.
    pub fn all_slash_commands(&self) -> Vec<&CommandFile> {
        self.categories
            .values()
            .filter(|category| category.enabled && matches!(category.status, CategoryStatus::Active))
            .flat_map(|category| category.commands.iter())
            .collect()
    }
}

fn compute_status(enabled: bool, errors: &[String]) -> CategoryStatus {
    if !errors.is_empty() {
        CategoryStatus::Error
    } else if enabled {
        CategoryStatus::Active
    } else {
        CategoryStatus::Inactive
    }
}

/// Load all skill categories from bundled assets generated at build time.
pub fn load_bundled_categories() -> Vec<SkillCategory> {
    BUNDLED_CATEGORIES
        .iter()
        .map(load_bundled_category)
        .collect()
}

fn load_bundled_category(bundled: &BundledCategory) -> SkillCategory {
    let id = bundled.id.to_string();
    let mut errors = Vec::new();

    let name = prettify_category_id(&id);

    let description = if bundled.readme.trim().is_empty() {
        errors.push(format!(
            "Bundled category '{id}' is missing README.md content (source files missing at build time?)"
        ));
        String::new()
    } else {
        extract_readme_description(bundled.readme)
    };

    let skills = bundled
        .skills
        .iter()
        .filter_map(|(relative_path, content)| {
            if content.trim().is_empty() {
                return None;
            }

            let path = bundled_path(&id, relative_path);
            Some(SkillFile::from_markdown(path, content))
        })
        .collect::<Vec<_>>();

    let commands = bundled
        .commands
        .iter()
        .filter_map(|(relative_path, content)| {
            if content.trim().is_empty() {
                return None;
            }

            let path = bundled_path(&id, relative_path);
            Some(CommandFile::from_markdown(path, id.clone(), content))
        })
        .collect::<Vec<_>>();

    let mcp_servers = if bundled.mcp_json.trim().is_empty() {
        HashMap::new()
    } else {
        match serde_json::from_str::<McpServersFile>(bundled.mcp_json) {
            Ok(file) => file.mcp_servers,
            Err(err) => {
                errors.push(format!("Failed parsing bundled MCP config for '{id}': {err}"));
                HashMap::new()
            }
        }
    };

    let connectors_doc = bundled.connectors_md.to_string();

    let enabled = false;
    let status = compute_status(enabled, &errors);

    SkillCategory {
        id,
        name,
        description,
        enabled,
        skills,
        commands,
        mcp_servers,
        connectors_doc,
        status,
        errors,
    }
}

fn bundled_path(category_id: &str, relative_path: &str) -> PathBuf {
    PathBuf::from("bundled")
        .join(category_id)
        .join(relative_path)
}

fn prettify_category_id(id: &str) -> String {
    id.split(|c| c == '-' || c == '_')
        .filter(|part| !part.trim().is_empty())
        .map(capitalize_word)
        .collect::<Vec<_>>()
        .join(" ")
}

fn capitalize_word(word: &str) -> String {
    let lower = word.trim().to_ascii_lowercase();
    let mut chars = lower.chars();

    match chars.next() {
        Some(first) => format!("{}{}", first.to_uppercase(), chars.collect::<String>()),
        None => String::new(),
    }
}

/// Extract the first paragraph following the first `# ...` heading.
///
/// Rules:
/// - find the first markdown heading starting with '#'
/// - take the following non-empty lines
/// - stop at the next blank line or next heading
fn extract_readme_description(readme: &str) -> String {
    let mut lines = readme.lines();

    // Find the first heading.
    for line in lines.by_ref() {
        if line.trim_start().starts_with('#') {
            break;
        }
    }

    let mut paragraph = Vec::new();

    for line in lines {
        let trimmed = line.trim_end();
        let is_blank = trimmed.trim().is_empty();
        let is_heading = trimmed.trim_start().starts_with('#');

        if is_heading {
            break;
        }

        if is_blank {
            if paragraph.is_empty() {
                continue;
            }
            break;
        }

        paragraph.push(trimmed);
    }

    paragraph.join("\n").trim().to_string()
}

// -----------------------------------------------------------------------------
// MCP bridge (simplified from the old mcp_bridge.rs)
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct McpBridgeResult {
    pub configs: HashMap<String, McpServerEntry>,
}

/// Builds a namespaced MCP server map from the provided categories.
///
/// Key format: "{category_id}:{server_name}".
pub fn build_mcp_map(categories: &[&SkillCategory]) -> McpBridgeResult {
    let mut result = McpBridgeResult::default();

    for category in categories {
        if !category.enabled {
            continue;
        }

        for (server_name, entry) in &category.mcp_servers {
            let namespaced = format!("{}:{server_name}", category.id);
            match resolve_entry_placeholders(entry) {
                Ok(resolved) => {
                    result.configs.insert(namespaced, resolved);
                }
                Err(_) => {
                    // Silently skip connectors with unresolved env vars.
                    // These are optional integrations (e.g. CLM, CRM, e-signature)
                    // that only become active when the user configures them.
                }
            }
        }
    }

    result
}

/// Resolves `${ENV_VAR}` placeholders inside MCP server entries.
///
/// If any placeholders cannot be resolved, returns `Err(...)` describing the missing vars.
pub fn resolve_entry_placeholders(entry: &McpServerEntry) -> Result<McpServerEntry, String> {
    let mut missing = Vec::new();

    let url = entry
        .url
        .as_deref()
        .map(|v| resolve_env_placeholders(v, &mut missing));
    let command = entry
        .command
        .as_deref()
        .map(|v| resolve_env_placeholders(v, &mut missing));
    let args = entry.args.as_ref().map(|items| {
        items
            .iter()
            .map(|item| resolve_env_placeholders(item, &mut missing))
            .collect::<Vec<_>>()
    });
    let env = entry.env.as_ref().map(|map| {
        map.iter()
            .map(|(k, v)| (k.clone(), resolve_env_placeholders(v, &mut missing)))
            .collect::<HashMap<_, _>>()
    });

    if !missing.is_empty() {
        missing.sort();
        missing.dedup();
        return Err(format!(
            "Connector is unavailable because these environment variables are missing: {}",
            missing.join(", ")
        ));
    }

    let resolved = McpServerEntry {
        r#type: entry.r#type.clone(),
        url,
        command,
        args,
        env,
    };

    validate_mcp_entry(&resolved)?;

    Ok(resolved)
}

fn resolve_env_placeholders(input: &str, missing: &mut Vec<String>) -> String {
    let bytes = input.as_bytes();
    let mut i = 0;
    let mut out = String::with_capacity(input.len());

    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            let start = i + 2;
            if let Some(end_rel) = input[start..].find('}') {
                let end = start + end_rel;
                let var_name = &input[start..end];
                match std::env::var(var_name) {
                    Ok(value) => out.push_str(&value),
                    Err(_) => {
                        missing.push(var_name.to_string());
                    }
                }
                i = end + 1;
                continue;
            }
        }

        out.push(bytes[i] as char);
        i += 1;
    }

    out
}

fn validate_mcp_entry(entry: &McpServerEntry) -> Result<(), String> {
    let kind = entry.r#type.trim().to_ascii_lowercase();

    match kind.as_str() {
        "http" => {
            if entry
                .url
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .is_empty()
            {
                return Err("Connector type `http` requires a non-empty `url` field".to_string());
            }
        }
        "stdio" => {
            if entry
                .command
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .is_empty()
            {
                return Err(
                    "Connector type `stdio` requires a non-empty `command` field".to_string(),
                );
            }
        }
        other => {
            return Err(format!(
                "Unsupported MCP connector type `{other}`. Expected `http` or `stdio`"
            ));
        }
    }

    Ok(())
}
