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

pub mod auth;
pub mod claude;
pub mod config;
pub mod db;
pub mod executor;
pub mod models;
pub mod prompts;
pub mod tools;

// Re-exports for convenience
pub use config::{model_display_name, Settings, Theme, DEFAULT_MODEL};
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
pub use executor::{event_channel, run_agent, EventReceiver, EventSender, ExecutorEvent, ImageData};

// Re-export image types for multimodal requests
pub use serdes_ai_core::messages::ImageMediaType;

// Re-export prompts
pub use prompts::{build_system_prompt, SYSTEM_PROMPT};

// Re-export tools
pub use tools::{
    DeleteFileTool, EditFileTool, FileError, GrepTool, ListFilesTool, ReadFileTool,
    RunShellCommandTool, ToolRegistry,
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
        let prompt = build_system_prompt(false, None);
        assert!(prompt.contains("Deskwork"));
    }
}
