//! Model configuration struct.

use serde::{Deserialize, Serialize};

use super::types::ModelType;

/// Configuration for a specific model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelConfig {
    /// Display name / identifier for the model (e.g., "claude-code-claude-sonnet-4-20250514")
    pub name: String,
    /// Provider type
    #[serde(default)]
    pub model_type: ModelType,
    /// The actual model ID to use in API calls (e.g., "claude-sonnet-4-20250514")
    #[serde(default)]
    pub model_id: Option<String>,
    /// Maximum context length in tokens
    #[serde(default = "default_context_length")]
    pub context_length: usize,
    /// Whether this model supports extended/deep thinking
    #[serde(default)]
    pub supports_thinking: bool,
    /// Whether this model supports vision/images
    #[serde(default = "default_true")]
    pub supports_vision: bool,
    /// Whether this model supports tool use/function calling
    #[serde(default = "default_true")]
    pub supports_tools: bool,
    /// Description of the model
    #[serde(default)]
    pub description: Option<String>,
}

fn default_context_length() -> usize {
    200_000
}

fn default_true() -> bool {
    true
}

impl Default for ModelConfig {
    fn default() -> Self {
        Self {
            name: "claude-code-claude-sonnet-4-20250514".to_string(),
            model_type: ModelType::ClaudeCode,
            model_id: Some("claude-sonnet-4-20250514".to_string()),
            context_length: default_context_length(),
            supports_thinking: true,
            supports_vision: true,
            supports_tools: true,
            description: None,
        }
    }
}

impl ModelConfig {
    /// Get the effective model ID for API calls.
    pub fn effective_model_id(&self) -> &str {
        self.model_id.as_deref().unwrap_or(&self.name)
    }

    /// Check if this is an OAuth-based model.
    pub fn is_oauth(&self) -> bool {
        matches!(self.model_type, ModelType::ClaudeCode)
    }

    /// Get a display-friendly name.
    pub fn display_name(&self) -> String {
        // Strip claude-code- prefix for display
        self.name
            .strip_prefix("claude-code-")
            .unwrap_or(&self.name)
            .to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_config_defaults() {
        let config = ModelConfig::default();
        assert_eq!(config.context_length, 200_000);
        assert!(config.supports_tools);
        assert!(config.supports_vision);
    }

    #[test]
    fn test_effective_model_id() {
        let mut config = ModelConfig::default();
        config.model_id = Some("claude-sonnet-4-20250514".to_string());
        assert_eq!(config.effective_model_id(), "claude-sonnet-4-20250514");

        config.model_id = None;
        assert_eq!(config.effective_model_id(), &config.name);
    }

    #[test]
    fn test_is_oauth() {
        let config = ModelConfig::default();
        assert!(config.is_oauth());
    }

    #[test]
    fn test_display_name() {
        let config = ModelConfig {
            name: "claude-code-claude-sonnet-4-20250514".to_string(),
            ..Default::default()
        };
        assert_eq!(config.display_name(), "claude-sonnet-4-20250514");
    }
}
