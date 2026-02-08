use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::plugins::context_builder::{build_plugin_prompt_block, ContextBudget, PluginContext};
use crate::plugins::mcp_bridge::{build_namespaced_mcp_map, McpBridgeResult};
use crate::plugins::registry::PluginRegistry;
use crate::plugins::slash_commands::{
    build_command_prompt, command_suggestions, command_suggestions_rich, parse_slash_command,
    ParsedSlashCommand, SlashCommandSuggestion,
};

#[derive(Debug, Clone)]
pub struct PluginRuntime {
    plugins_dir: PathBuf,
    enabled_ids: Vec<String>,
    context_budget: ContextBudget,
    registry: PluginRegistry,
    mcp: McpBridgeResult,
}

impl PluginRuntime {
    pub fn new(plugins_dir: impl AsRef<Path>, enabled_ids: &[String]) -> Self {
        let plugins_dir = plugins_dir.as_ref().to_path_buf();
        let registry = PluginRegistry::load(&plugins_dir, enabled_ids);
        let mcp = build_namespaced_mcp_map(registry.enabled_plugins());

        Self {
            plugins_dir,
            enabled_ids: enabled_ids.to_vec(),
            context_budget: ContextBudget::default(),
            registry,
            mcp,
        }
    }

    pub fn load(&mut self) {
        self.registry = PluginRegistry::load(&self.plugins_dir, &self.enabled_ids);
        self.rebuild_mcp();
    }

    pub fn reload(&mut self) {
        self.registry.reload(&self.plugins_dir, &self.enabled_ids);
        self.rebuild_mcp();
    }

    pub fn set_context_budget(&mut self, max_tokens: usize) {
        self.context_budget = ContextBudget { max_tokens };
    }

    pub fn enable_plugin(&mut self, plugin_id: &str) {
        if !self.enabled_ids.iter().any(|id| id == plugin_id) {
            self.enabled_ids.push(plugin_id.to_string());
        }

        self.registry.enable(plugin_id);
        self.rebuild_mcp();
    }

    pub fn disable_plugin(&mut self, plugin_id: &str) {
        self.enabled_ids.retain(|id| id != plugin_id);
        self.registry.disable(plugin_id);
        self.rebuild_mcp();
    }

    pub fn toggle_plugin(&mut self, plugin_id: &str, enabled: bool) {
        if enabled {
            self.enable_plugin(plugin_id);
        } else {
            self.disable_plugin(plugin_id);
        }
    }

    pub fn plugin_context(&self) -> PluginContext {
        build_plugin_prompt_block(&self.registry, &self.mcp, self.context_budget)
    }

    pub fn parse_command(&self, input: &str) -> Option<ParsedSlashCommand> {
        parse_slash_command(input)
    }

    pub fn command_suggestions(&self, prefix: &str) -> Vec<String> {
        command_suggestions(&self.registry, prefix)
    }

    pub fn command_suggestions_rich(&self, prefix: &str) -> Vec<SlashCommandSuggestion> {
        command_suggestions_rich(&self.registry, prefix)
    }

    pub fn execute_command(
        &self,
        input: &str,
        user_inputs: &HashMap<String, String>,
    ) -> Result<String, String> {
        let parsed = parse_slash_command(input).ok_or_else(|| {
            "Input is not a valid slash command. Expected format: /{plugin}:{command}".to_string()
        })?;

        let command = self
            .registry
            .get_command_handler(&parsed.slash_command)
            .ok_or_else(|| {
                format!(
                    "No enabled command handler found for {}",
                    parsed.slash_command
                )
            })?;

        Ok(build_command_prompt(
            command,
            user_inputs,
            parsed.raw_args.as_deref(),
        ))
    }

    pub fn registry(&self) -> &PluginRegistry {
        &self.registry
    }

    pub fn mcp_bridge_result(&self) -> &McpBridgeResult {
        &self.mcp
    }

    fn rebuild_mcp(&mut self) {
        self.mcp = build_namespaced_mcp_map(self.registry.enabled_plugins());
    }
}
