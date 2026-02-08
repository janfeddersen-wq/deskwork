pub mod bundled;
pub mod context_builder;
pub mod loader;
pub mod mcp_bridge;
pub mod mcp_manager;
pub mod mcp_tool;
pub mod registry;
pub mod runtime;
pub mod slash_commands;
pub mod types;

pub use bundled::{load_all_bundled_plugins, load_bundled_legal_plugin};
pub use context_builder::{build_plugin_prompt_block, ContextBudget, PluginContext};
pub use loader::{discover_plugins, load_bundled_plugins, load_plugin, PluginLoadError};
pub use mcp_bridge::{
    build_namespaced_mcp_map, resolve_entry_placeholders, McpBridgeResult, UnavailableConnector,
};
pub use mcp_manager::{mcp_tool_key, NamespacedMcpTool, PluginMcpManager};
pub use mcp_tool::PluginMcpTool;
pub use registry::PluginRegistry;
pub use runtime::PluginRuntime;
pub use slash_commands::{
    build_command_prompt, command_suggestions, command_suggestions_rich, parse_slash_command,
    ParsedSlashCommand, SlashCommandSuggestion,
};
pub use types::{
    parse_frontmatter, CommandFile, CommandFrontmatter, McpServerEntry, McpServersFile, Plugin,
    PluginAuthor, PluginManifest, PluginStatus, SkillFile, SkillFrontmatter,
};
