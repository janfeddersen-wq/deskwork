use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::plugins::loader::{discover_plugins, load_bundled_plugins};
use crate::plugins::types::{CommandFile, McpServerEntry, Plugin, PluginStatus, SkillFile};

#[derive(Debug, Default, Clone)]
pub struct PluginRegistry {
    plugins: HashMap<String, Plugin>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    pub fn load(plugins_dir: &Path, enabled_ids: &[String]) -> Self {
        let mut registry = Self::new();

        for mut plugin in load_bundled_plugins() {
            hydrate_bundled_local_config(&mut plugin, plugins_dir);
            registry.plugins.insert(plugin.id.clone(), plugin);
        }

        for plugin in discover_plugins(plugins_dir).into_iter().flatten() {
            // Disk plugin definitions override bundled plugin IDs.
            registry.plugins.insert(plugin.id.clone(), plugin);
        }

        let enabled: std::collections::HashSet<&str> =
            enabled_ids.iter().map(String::as_str).collect();

        for plugin in registry.plugins.values_mut() {
            plugin.enabled = enabled.contains(plugin.id.as_str());
            plugin.status = if !plugin.errors.is_empty() {
                PluginStatus::Error
            } else if plugin.enabled {
                PluginStatus::Active
            } else {
                PluginStatus::Inactive
            };
        }

        registry
    }

    pub fn enable(&mut self, plugin_id: &str) {
        if let Some(plugin) = self.plugins.get_mut(plugin_id) {
            plugin.enabled = true;
            plugin.status = if plugin.errors.is_empty() {
                PluginStatus::Active
            } else {
                PluginStatus::Error
            };
        }
    }

    pub fn disable(&mut self, plugin_id: &str) {
        if let Some(plugin) = self.plugins.get_mut(plugin_id) {
            plugin.enabled = false;
            plugin.status = if plugin.errors.is_empty() {
                PluginStatus::Inactive
            } else {
                PluginStatus::Error
            };
        }
    }

    pub fn get_plugin(&self, plugin_id: &str) -> Option<&Plugin> {
        self.plugins.get(plugin_id)
    }

    pub fn get_command_handler(&self, slash_command: &str) -> Option<&CommandFile> {
        self.plugins
            .values()
            .filter(|plugin| plugin.enabled && matches!(plugin.status, PluginStatus::Active))
            .flat_map(|plugin| plugin.commands.iter())
            .find(|command| command.slash_command == slash_command)
    }

    pub fn get_active_skills(&self) -> Vec<&SkillFile> {
        self.plugins
            .values()
            .filter(|plugin| plugin.enabled && matches!(plugin.status, PluginStatus::Active))
            .flat_map(|plugin| plugin.skills.iter())
            .collect()
    }

    pub fn get_active_mcp_configs(&self) -> HashMap<String, &McpServerEntry> {
        let mut configs = HashMap::new();

        for plugin in self
            .plugins
            .values()
            .filter(|plugin| plugin.enabled && matches!(plugin.status, PluginStatus::Active))
        {
            for (server_name, server) in &plugin.mcp_servers {
                let key = format!("{}:{}", plugin.id, server_name);
                configs.insert(key, server);
            }
        }

        configs
    }

    pub fn reload(&mut self, plugins_dir: &Path, enabled_ids: &[String]) {
        *self = Self::load(plugins_dir, enabled_ids);
    }

    pub fn enabled_plugins(&self) -> Vec<&Plugin> {
        self.plugins
            .values()
            .filter(|plugin| plugin.enabled && matches!(plugin.status, PluginStatus::Active))
            .collect()
    }

    pub fn all_plugins(&self) -> Vec<&Plugin> {
        self.plugins.values().collect()
    }

    pub fn all_slash_commands(&self) -> Vec<&CommandFile> {
        self.plugins
            .values()
            .filter(|plugin| plugin.enabled && matches!(plugin.status, PluginStatus::Active))
            .flat_map(|plugin| plugin.commands.iter())
            .collect()
    }
}

fn hydrate_bundled_local_config(plugin: &mut Plugin, plugins_dir: &Path) {
    let is_bundled = plugin.path.to_string_lossy().starts_with("bundled/");
    if !is_bundled || plugin.id != "legal" {
        return;
    }

    let local_dir = plugins_dir.join(&plugin.id);
    let local_path = local_dir.join(format!("{}.local.md", plugin.id));

    if !local_path.exists() {
        if let Err(err) = fs::create_dir_all(&local_dir) {
            plugin.errors.push(format!(
                "Failed creating plugin local config directory '{}': {}",
                local_dir.display(),
                err
            ));
            return;
        }

        if let Some(default_local) = plugin.local_config.as_deref() {
            if let Err(err) = fs::write(&local_path, default_local) {
                plugin.errors.push(format!(
                    "Failed writing default bundled local config '{}': {}",
                    local_path.display(),
                    err
                ));
                return;
            }
        }
    }

    match fs::read_to_string(&local_path) {
        Ok(content) => plugin.local_config = Some(content),
        Err(err) => plugin.errors.push(format!(
            "Failed reading bundled local config '{}': {}",
            local_path.display(),
            err
        )),
    }
}
