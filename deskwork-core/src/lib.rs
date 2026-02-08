//! Deskwork Core Library
//!
//! This crate provides the core functionality for Deskwork, a Claude-powered
//! coding assistant. It includes:
//!
//! - OAuth authentication with Claude
//! - Model registry for storing model configurations
//! - Configuration management (settings, preferences)
//! - Database layer for tokens, settings, and conversations
//! - Tool implementations (file operations, shell commands, etc.)
//! - Claude model integration via serdes-ai
//! - Agent executor for running Claude with tools
//! - System prompts for the coding assistant
//! - External tools management (UV download and installation)
//! - Python environment management (venv creation, package installation)

pub mod auth;
pub mod claude;
pub mod config;
pub mod db;
pub mod executor;
pub mod external_tools;
pub mod models;
pub mod plugins;
pub mod prompts;
pub mod python;
pub mod skills;
pub mod tools;

// Re-exports for convenience
pub use config::{model_display_name, RenderMode, Settings, Theme, DEFAULT_MODEL};
pub use db::Database;

// Re-export auth
pub use auth::{
    fetch_claude_models, filter_latest_models, get_claude_code_model, has_oauth_tokens,
    run_claude_code_auth, save_claude_models_to_db, ClaudeCodeAuth, ClaudeCodeAuthError,
    StoredTokens, TokenStorage, TokenStorageError,
};

// Re-export models
pub use models::{ModelConfig, ModelConfigError, ModelRegistry, ModelType};

// Re-export Claude integration
pub use claude::{create_model, create_model_with_thinking};

// Re-export executor
pub use executor::{
    event_channel, run_agent, EventReceiver, EventSender, ExecutorEvent, ImageData, RunAgentArgs,
};

// Re-export image types for multimodal requests
pub use serdes_ai_core::messages::ImageMediaType;

// Re-export prompts
pub use prompts::{build_system_prompt, SYSTEM_PROMPT};

// Re-export tools
pub use tools::{
    DeleteFileTool, EditFileTool, FileError, GrepTool, ListFilesTool, ReadFileTool,
    RunShellCommandTool, ToolRegistry,
};

// Re-export plugins
pub use plugins::{
    build_command_prompt, build_namespaced_mcp_map, build_plugin_prompt_block, command_suggestions,
    command_suggestions_rich, discover_plugins, load_all_bundled_plugins,
    load_bundled_legal_plugin, load_bundled_plugins, load_plugin, parse_frontmatter,
    parse_slash_command, resolve_entry_placeholders, CommandFile, CommandFrontmatter,
    ContextBudget, McpBridgeResult, McpServerEntry, McpServersFile, NamespacedMcpTool,
    ParsedSlashCommand, Plugin, PluginAuthor, PluginContext, PluginLoadError, PluginManifest,
    PluginMcpManager, PluginMcpTool, PluginRegistry, PluginRuntime, PluginStatus, SkillFile,
    SkillFrontmatter, SlashCommandSuggestion, UnavailableConnector,
};

// Re-export skills
pub use skills::{
    discover_skills, extract_skills_if_needed, get_skill_path, SkillMetadata, SkillsContext,
};

// Re-export external tools
pub use external_tools::{ExternalToolId, ExternalToolManager, Platform, ToolStatus};

// Re-export python utilities
pub use python::{
    create_venv, ensure_venv, get_venv_python, is_uv_installed, pip_install,
    pip_install_requirements, run_python_module, run_python_script,
};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_set() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn exports_are_accessible() {
        // Verify all public types are accessible
        fn _check_types(
            _db: &Database,
            _settings: &Settings,
            _theme: Theme,
            _list_tool: &ListFilesTool,
            _read_tool: &ReadFileTool,
            _edit_tool: &EditFileTool,
            _delete_tool: &DeleteFileTool,
            _grep_tool: &GrepTool,
            _shell_tool: &RunShellCommandTool,
            _registry: &ToolRegistry,
            _model_config: &ModelConfig,
            _model_registry: &ModelRegistry,
        ) {
        }
    }

    #[test]
    fn prompts_exported() {
        assert!(!SYSTEM_PROMPT.is_empty());
        let prompt = build_system_prompt(false, None, None, None);
        assert!(prompt.contains("Deskwork"));
    }

    #[test]
    fn external_tools_module_accessible() {
        // Verify external tools types are accessible
        let _all = ExternalToolId::all();
        let _platform = Platform::detect();
    }
}
