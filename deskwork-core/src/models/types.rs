//! Core model type definitions.

use std::str::FromStr;

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

/// Error returned when parsing a [`ModelType`] from a string.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("Unknown model type: {0}")]
pub struct ModelTypeParseError(String);

/// Supported model provider types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModelType {
    /// Claude Code OAuth-authenticated.
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
    /// Parse from string, defaulting to [`ModelType::ClaudeCode`] for unknown values.
    pub fn parse_lossy(s: &str) -> Self {
        match s {
            "claude_code" | "claude-code" => ModelType::ClaudeCode,
            _ => ModelType::ClaudeCode,
        }
    }
}

impl FromStr for ModelType {
    type Err = ModelTypeParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "claude_code" | "claude-code" => Ok(ModelType::ClaudeCode),
            _ => Err(ModelTypeParseError(s.to_string())),
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
    fn test_model_type_parse_lossy() {
        assert_eq!(ModelType::parse_lossy("claude_code"), ModelType::ClaudeCode);
        assert_eq!(ModelType::parse_lossy("claude-code"), ModelType::ClaudeCode);
        assert_eq!(ModelType::parse_lossy("unknown"), ModelType::ClaudeCode);
    }

    #[test]
    fn test_model_type_from_str_trait() {
        let parsed: ModelType = "claude_code".parse().unwrap();
        assert_eq!(parsed, ModelType::ClaudeCode);
    }
}
