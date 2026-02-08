//! Claude Code OAuth model configuration.
//!
//! Thin wrapper around serdes-ai-models to create models with OAuth tokens.
//!
//! **Important**: Temperature and max_tokens are NOT set on the model.
//! They are passed via `ModelSettings` at request time through the agent builder.

use crate::config::Settings;
use serdes_ai_models::claude_code_oauth::ClaudeCodeOAuthModel;

/// Strip the claude-code- prefix from model names.
///
/// Model names in the registry are prefixed (e.g., "claude-code-claude-sonnet-4-20250514")
/// but the API expects just the model ID (e.g., "claude-sonnet-4-20250514").
fn strip_model_prefix(model_name: &str) -> &str {
    model_name
        .strip_prefix("claude-code-")
        .or_else(|| model_name.strip_prefix("claude_code_"))
        .unwrap_or(model_name)
}

/// Create a Claude Code OAuth model instance.
///
/// This configures the model based on user preferences including:
/// - Model name (dynamic, from API)
/// - Extended thinking mode and budget
///
/// **Note**: Temperature is NOT set here - it's passed via the agent builder
/// which uses `ModelSettings` internally.
///
/// # Arguments
///
/// * `model_name` - The model ID (e.g., "claude-code-claude-sonnet-4-20250514" or "claude-sonnet-4-20250514")
/// * `access_token` - OAuth access token from authentication flow
/// * `settings` - User settings from the database
///
/// # Example
///
/// ```ignore
/// use deskwork_core::claude::create_model;
/// use deskwork_core::Settings;
///
/// let settings = Settings::default();
/// let model = create_model("claude-code-claude-sonnet-4-20250514", "access_token", &settings);
/// ```
pub fn create_model(
    model_name: &str,
    access_token: &str,
    settings: &Settings,
) -> ClaudeCodeOAuthModel {
    let actual_model_name = strip_model_prefix(model_name);
    let mut model = ClaudeCodeOAuthModel::new(actual_model_name, access_token);

    // Extended thinking mode (for complex reasoning tasks)
    if settings.extended_thinking {
        model = model.with_thinking(Some(settings.thinking_budget as u64));
    }

    model
}

/// Create a Claude Code OAuth model with explicit thinking config.
///
/// Useful when you want to override the settings.
pub fn create_model_with_thinking(
    model_name: &str,
    access_token: &str,
    thinking_budget: Option<u64>,
) -> ClaudeCodeOAuthModel {
    let actual_model_name = strip_model_prefix(model_name);
    let mut model = ClaudeCodeOAuthModel::new(actual_model_name, access_token);

    if let Some(budget) = thinking_budget {
        model = model.with_thinking(Some(budget));
    }

    model
}

#[cfg(test)]
mod tests {
    use super::*;
    use serdes_ai_models::Model;

    #[test]
    fn test_strip_model_prefix() {
        assert_eq!(
            strip_model_prefix("claude-code-claude-sonnet-4-20250514"),
            "claude-sonnet-4-20250514"
        );
        assert_eq!(
            strip_model_prefix("claude_code_claude-sonnet-4-20250514"),
            "claude-sonnet-4-20250514"
        );
        assert_eq!(
            strip_model_prefix("claude-sonnet-4-20250514"),
            "claude-sonnet-4-20250514"
        );
    }

    #[test]
    fn test_create_model_default_settings() {
        let settings = Settings::default();
        let model = create_model(
            "claude-code-claude-sonnet-4-20250514",
            "test-token",
            &settings,
        );
        // Model name should have prefix stripped
        assert_eq!(model.name(), "claude-sonnet-4-20250514");
    }

    #[test]
    fn test_create_model_with_thinking() {
        let mut settings = Settings::default();
        settings.extended_thinking = true;
        settings.thinking_budget = 10000;
        let model = create_model(
            "claude-code-claude-sonnet-4-20250514",
            "test-token",
            &settings,
        );
        assert_eq!(model.name(), "claude-sonnet-4-20250514");
    }

    #[test]
    fn test_create_model_with_explicit_thinking() {
        let model = create_model_with_thinking(
            "claude-code-claude-opus-4-20250514",
            "test-token",
            Some(16000),
        );
        assert_eq!(model.name(), "claude-opus-4-20250514");
    }
}
