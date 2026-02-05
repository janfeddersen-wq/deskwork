//! Core model type definitions.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during model configuration.
#[derive(Debug, Error)]
pub enum ModelConfigError {
    #[error("Database error: {0}")]
    Database(String),
    #[error("Model not found: {0}")]
    ModelNotFound(String),
}

/// Supported model provider types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModelType {
    /// Claude Code OAuth-authenticated
    #[default]
    ClaudeCode,
}

impl std::fmt::Display for ModelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelType::ClaudeCode => write!(f, "claude_code"),
        }
    }
}

impl ModelType {
    /// Parse from string.
    pub fn from_str(s: &str) -> Self {
        match s {
            "claude_code" | "claude-code" => ModelType::ClaudeCode,
            _ => ModelType::ClaudeCode,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_type_display() {
        assert_eq!(ModelType::ClaudeCode.to_string(), "claude_code");
    }

    #[test]
    fn test_model_type_from_str() {
        assert_eq!(ModelType::from_str("claude_code"), ModelType::ClaudeCode);
        assert_eq!(ModelType::from_str("claude-code"), ModelType::ClaudeCode);
    }
}
