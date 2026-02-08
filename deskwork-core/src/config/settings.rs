//! Application settings for Deskwork.
//!
//! Settings are persisted to the SQLite database as JSON.

use serde::{Deserialize, Serialize};

// =============================================================================
// Theme Selection
// =============================================================================

/// App theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Theme {
    /// Dark theme (easier on the eyes)
    #[default]
    Dark,
    /// Light theme
    Light,
}

impl Theme {
    /// Get all available themes.
    pub fn all() -> &'static [Theme] {
        &[Self::Dark, Self::Light]
    }
}

impl std::fmt::Display for Theme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dark => write!(f, "Dark"),
            Self::Light => write!(f, "Light"),
        }
    }
}

// =============================================================================
// Default Model
// =============================================================================

/// Default model to use when none is selected.
/// Uses prefixed name for consistency with registry.
pub const DEFAULT_MODEL: &str = "claude-code-claude-sonnet-4-20250514";
pub const DEFAULT_PLUGIN_CONTEXT_TOKEN_BUDGET: u32 = 6000;

/// Default list of enabled skill category IDs.
///
/// We keep this helper because serde's `default = "..."` needs a function.
fn default_plugins_enabled() -> Vec<String> {
    Vec::new()
}

fn default_plugin_context_token_budget() -> u32 {
    DEFAULT_PLUGIN_CONTEXT_TOKEN_BUDGET
}

fn default_stream_markdown_enabled() -> bool {
    true
}

/// Get display name for a model ID.
///
/// Handles prefixed names like "claude-code-claude-sonnet-4-20250514".
/// Examples:
///   - `claude-sonnet-4-5-20250929` -> "Claude 4.5 Sonnet"
///   - `claude-opus-4-5-20251101` -> "Claude 4.5 Opus"
///   - `claude-sonnet-4-20250514` -> "Claude 4 Sonnet"
///   - `claude-3-5-sonnet-20241022` -> "Claude 3.5 Sonnet"
pub fn model_display_name(model_id: &str) -> String {
    // Strip claude-code- prefix if present
    let stripped = model_id.strip_prefix("claude-code-").unwrap_or(model_id);

    // Determine family
    let family = if stripped.contains("sonnet") {
        "Sonnet"
    } else if stripped.contains("opus") {
        "Opus"
    } else if stripped.contains("haiku") {
        "Haiku"
    } else {
        return stripped.to_string();
    };

    // Extract version numbers (exclude long numbers like dates)
    let numbers: Vec<u32> = stripped
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty() && s.len() <= 2) // Only short numbers (not dates)
        .filter_map(|s| s.parse().ok())
        .collect();

    // Format based on version pattern
    match numbers.as_slice() {
        [major, minor] if *major >= 3 => format!("Claude {}.{} {}", major, minor, family),
        [major] if *major >= 3 => format!("Claude {} {}", major, family),
        _ => format!("Claude {}", family),
    }
}

// =============================================================================
// Render Mode
// =============================================================================

/// Rendering mode for the application.
///
/// Controls whether the app uses GPU hardware acceleration or falls back
/// to CPU-based software rendering. Software mode is useful for terminal
/// servers and virtual machines without GPU access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RenderMode {
    /// Prefer GPU acceleration, fall back to software if unavailable.
    #[default]
    Auto,
    /// Force CPU-based software rendering (no GPU required).
    Software,
}

impl RenderMode {
    /// Get all available render modes.
    pub fn all() -> &'static [RenderMode] {
        &[Self::Auto, Self::Software]
    }
}

impl std::fmt::Display for RenderMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Auto => write!(f, "Auto (GPU preferred)"),
            Self::Software => write!(f, "Software (CPU only)"),
        }
    }
}

// =============================================================================
// Application Settings
// =============================================================================

/// Application settings - persisted to database as JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Selected model name (e.g., "claude-sonnet-4-20250514").
    /// Dynamically fetched from API after OAuth.
    pub model: String,

    /// Available models (fetched from API).
    #[serde(default)]
    pub available_models: Vec<String>,

    /// Max tokens for response.
    pub max_tokens: u32,

    /// Temperature (0.0 - 1.0) - higher = more creative.
    pub temperature: f32,

    /// Enable extended thinking mode.
    pub extended_thinking: bool,

    /// Budget tokens for thinking (when enabled).
    pub thinking_budget: u32,

    /// UI theme.
    pub theme: Theme,

    /// Rendering mode (GPU vs software).
    /// Requires app restart to take effect.
    #[serde(default)]
    pub render_mode: RenderMode,

    /// Show thinking process in UI.
    pub show_thinking: bool,

    /// Render streamed assistant output as markdown.
    #[serde(default = "default_stream_markdown_enabled")]
    pub stream_markdown_enabled: bool,

    /// Working directory for file operations.
    pub working_directory: Option<String>,

    /// List of enabled skill category IDs (historically named "plugins_enabled" for settings backward compatibility).
    #[serde(default = "default_plugins_enabled")]
    pub plugins_enabled: Vec<String>,

    /// Token budget reserved for skill category context in prompts.
    #[serde(default = "default_plugin_context_token_budget")]
    pub plugin_context_token_budget: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model: DEFAULT_MODEL.to_string(),
            available_models: Vec::new(),
            // NOTE: max_tokens and temperature are NOT used for Claude Code OAuth
            // They are hardcoded in the executor (30000 tokens, temp 1.0)
            // Keeping these fields for potential future non-OAuth model support
            max_tokens: 30000,
            temperature: 1.0,
            // Extended thinking is always enabled for Claude Code OAuth
            extended_thinking: true,
            thinking_budget: 10000,
            theme: Theme::default(),
            render_mode: RenderMode::default(),
            show_thinking: true,
            stream_markdown_enabled: default_stream_markdown_enabled(),
            working_directory: None,
            plugins_enabled: default_plugins_enabled(),
            plugin_context_token_budget: default_plugin_context_token_budget(),
        }
    }
}

impl Settings {
    /// Load settings from database, using defaults for missing values.
    ///
    /// If settings don't exist or can't be parsed, returns defaults.
    pub fn load(db: &crate::db::Database) -> Self {
        let mut settings = Self::default();

        if let Ok(Some(json)) = db.get_setting("settings") {
            match serde_json::from_str::<Settings>(&json) {
                Ok(loaded) => settings = loaded,
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to parse settings, using defaults");
                }
            }
        }

        settings
    }

    /// Save settings to database.
    pub fn save(&self, db: &crate::db::Database) -> anyhow::Result<()> {
        let json = serde_json::to_string(self)?;
        db.set_setting("settings", &json)?;
        Ok(())
    }

    /// Validate and clamp settings to valid ranges.
    pub fn validate(&mut self) {
        // Clamp temperature to valid range
        self.temperature = self.temperature.clamp(0.0, 1.0);

        // Ensure reasonable token limits
        self.max_tokens = self.max_tokens.clamp(256, 32768);
        self.thinking_budget = self.thinking_budget.clamp(1000, 100000);
        self.plugin_context_token_budget = self.plugin_context_token_budget.clamp(500, 100_000);

        // Ensure model is set
        if self.model.is_empty() {
            self.model = DEFAULT_MODEL.to_string();
        }
    }

    /// Get display name for the current model.
    pub fn model_display_name(&self) -> String {
        model_display_name(&self.model)
    }

    /// Set available models and update current model if needed.
    pub fn set_available_models(&mut self, models: Vec<String>) {
        self.available_models = models;

        // If current model isn't in the list, use first available
        if !self.available_models.is_empty() && !self.available_models.contains(&self.model) {
            // Try to find a sonnet model first
            if let Some(sonnet) = self.available_models.iter().find(|m| m.contains("sonnet")) {
                self.model = sonnet.clone();
            } else {
                self.model = self.available_models[0].clone();
            }
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_test_db() -> (TempDir, crate::db::Database) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = crate::db::Database::open_at(db_path).unwrap();
        db.migrate().unwrap();
        (temp_dir, db)
    }

    // -------------------------------------------------------------------------
    // Theme Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_theme_default_is_dark() {
        assert_eq!(Theme::default(), Theme::Dark);
    }

    #[test]
    fn test_theme_all() {
        let all = Theme::all();
        assert_eq!(all.len(), 2);
        assert!(all.contains(&Theme::Dark));
        assert!(all.contains(&Theme::Light));
    }

    #[test]
    fn test_theme_serialization() {
        let json = serde_json::to_string(&Theme::Light).unwrap();
        let parsed: Theme = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, Theme::Light);
    }

    // -------------------------------------------------------------------------
    // RenderMode Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_render_mode_default_is_auto() {
        assert_eq!(RenderMode::default(), RenderMode::Auto);
    }

    #[test]
    fn test_render_mode_all() {
        let all = RenderMode::all();
        assert_eq!(all.len(), 2);
        assert!(all.contains(&RenderMode::Auto));
        assert!(all.contains(&RenderMode::Software));
    }

    #[test]
    fn test_render_mode_serialization() {
        let json = serde_json::to_string(&RenderMode::Software).unwrap();
        let parsed: RenderMode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, RenderMode::Software);
    }

    #[test]
    fn test_render_mode_display() {
        assert!(RenderMode::Auto.to_string().contains("GPU"));
        assert!(RenderMode::Software.to_string().contains("CPU"));
    }

    #[test]
    fn test_settings_render_mode_persists() {
        let (_temp, db) = setup_test_db();

        let mut settings = Settings::default();
        settings.render_mode = RenderMode::Software;
        settings.save(&db).unwrap();

        let loaded = Settings::load(&db);
        assert_eq!(loaded.render_mode, RenderMode::Software);
    }

    #[test]
    fn test_settings_render_mode_defaults_on_missing() {
        let (_temp, db) = setup_test_db();

        // Save settings JSON without render_mode field (simulates old config)
        let json = r#"{"model":"test","max_tokens":4096,"temperature":0.7,"extended_thinking":false,"thinking_budget":10000,"theme":"Dark","show_thinking":true}"#;
        db.set_setting("settings", json).unwrap();

        let loaded = Settings::load(&db);
        assert_eq!(loaded.render_mode, RenderMode::Auto);
        assert!(loaded.stream_markdown_enabled);
    }

    // -------------------------------------------------------------------------
    // Model Display Name Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_model_display_name() {
        // 4.5 models (newest format: claude-<family>-4-5-YYYYMMDD)
        assert_eq!(
            model_display_name("claude-sonnet-4-5-20250929"),
            "Claude 4.5 Sonnet"
        );
        assert_eq!(
            model_display_name("claude-opus-4-5-20251101"),
            "Claude 4.5 Opus"
        );
        // Prefixed 4.5 models
        assert_eq!(
            model_display_name("claude-code-claude-sonnet-4-5-20250929"),
            "Claude 4.5 Sonnet"
        );

        // 4.0 models (format: claude-<family>-4-YYYYMMDD)
        assert_eq!(
            model_display_name("claude-sonnet-4-20250514"),
            "Claude 4 Sonnet"
        );
        assert_eq!(
            model_display_name("claude-code-claude-sonnet-4-20250514"),
            "Claude 4 Sonnet"
        );

        // 3.5 models (format: claude-3-5-<family>-YYYYMMDD)
        assert_eq!(
            model_display_name("claude-3-5-sonnet-20241022"),
            "Claude 3.5 Sonnet"
        );
        assert_eq!(
            model_display_name("claude-code-claude-3-5-haiku-20241022"),
            "Claude 3.5 Haiku"
        );

        // 3.0 models (format: claude-3-<family>-YYYYMMDD)
        assert_eq!(
            model_display_name("claude-3-opus-20240229"),
            "Claude 3 Opus"
        );

        // Unknown models return as-is
        assert_eq!(model_display_name("unknown-model"), "unknown-model");
    }

    // -------------------------------------------------------------------------
    // Settings Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_settings_default() {
        let settings = Settings::default();
        assert_eq!(settings.model, DEFAULT_MODEL);
        // Claude Code OAuth hardcoded values
        assert_eq!(settings.max_tokens, 30000);
        assert!((settings.temperature - 1.0).abs() < f32::EPSILON);
        assert!(settings.extended_thinking);
        assert_eq!(settings.thinking_budget, 10000);
        assert_eq!(settings.theme, Theme::Dark);
        assert!(settings.show_thinking);
        assert!(settings.stream_markdown_enabled);
        assert!(settings.working_directory.is_none());
        assert!(settings.available_models.is_empty());
        assert!(settings.plugins_enabled.is_empty());
        assert_eq!(
            settings.plugin_context_token_budget,
            DEFAULT_PLUGIN_CONTEXT_TOKEN_BUDGET
        );
    }

    #[test]
    fn test_settings_save_and_load_roundtrip() {
        let (_temp, db) = setup_test_db();

        // Create custom settings
        let mut original = Settings::default();
        original.model = "claude-3-opus-20240229".to_string();
        original.max_tokens = 16384;
        original.temperature = 0.5;
        original.theme = Theme::Light;
        original.stream_markdown_enabled = false;
        original.working_directory = Some("/home/user/project".to_string());
        original.plugins_enabled = vec!["legal".to_string()];
        original.plugin_context_token_budget = 7000;

        // Save
        original.save(&db).unwrap();

        // Load
        let loaded = Settings::load(&db);

        // Verify
        assert_eq!(loaded.model, "claude-3-opus-20240229");
        assert_eq!(loaded.max_tokens, 16384);
        assert!((loaded.temperature - 0.5).abs() < f32::EPSILON);
        assert_eq!(loaded.theme, Theme::Light);
        assert!(!loaded.stream_markdown_enabled);
        assert_eq!(
            loaded.working_directory,
            Some("/home/user/project".to_string())
        );
        assert_eq!(loaded.plugins_enabled, vec!["legal".to_string()]);
        assert_eq!(loaded.plugin_context_token_budget, 7000);
    }

    #[test]
    fn test_settings_load_returns_defaults_when_missing() {
        let (_temp, db) = setup_test_db();

        // Don't save anything - should get defaults
        let settings = Settings::load(&db);

        assert_eq!(settings.model, DEFAULT_MODEL);
        assert_eq!(settings.theme, Theme::default());
    }

    #[test]
    fn test_settings_load_returns_defaults_on_invalid_json() {
        let (_temp, db) = setup_test_db();

        // Save invalid JSON
        db.set_setting("settings", "not valid json {{").unwrap();

        // Should get defaults
        let settings = Settings::load(&db);
        assert_eq!(settings.model, DEFAULT_MODEL);
    }

    #[test]
    fn test_settings_validate_clamps_temperature() {
        let mut settings = Settings::default();

        settings.temperature = -0.5;
        settings.validate();
        assert!((settings.temperature - 0.0).abs() < f32::EPSILON);

        settings.temperature = 1.5;
        settings.validate();
        assert!((settings.temperature - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_settings_validate_clamps_max_tokens() {
        let mut settings = Settings::default();

        settings.max_tokens = 100;
        settings.validate();
        assert_eq!(settings.max_tokens, 256);

        settings.max_tokens = 100000;
        settings.validate();
        assert_eq!(settings.max_tokens, 32768);
    }

    #[test]
    fn test_settings_validate_clamps_thinking_budget() {
        let mut settings = Settings::default();

        settings.thinking_budget = 500;
        settings.validate();
        assert_eq!(settings.thinking_budget, 1000);

        settings.thinking_budget = 500000;
        settings.validate();
        assert_eq!(settings.thinking_budget, 100000);
    }

    #[test]
    fn test_settings_validate_sets_empty_model() {
        let mut settings = Settings::default();
        settings.model = String::new();
        settings.validate();
        assert_eq!(settings.model, DEFAULT_MODEL);
    }

    #[test]
    fn test_settings_validate_clamps_plugin_context_budget() {
        let mut settings = Settings::default();

        settings.plugin_context_token_budget = 100;
        settings.validate();
        assert_eq!(settings.plugin_context_token_budget, 500);

        settings.plugin_context_token_budget = 1_000_000;
        settings.validate();
        assert_eq!(settings.plugin_context_token_budget, 100_000);
    }

    #[test]
    fn test_settings_plugin_fields_default_on_missing() {
        let (_temp, db) = setup_test_db();

        let json = r#"{"model":"test","max_tokens":4096,"temperature":0.7,"extended_thinking":false,"thinking_budget":10000,"theme":"Dark","show_thinking":true,"render_mode":"Auto"}"#;
        db.set_setting("settings", json).unwrap();

        let loaded = Settings::load(&db);
        assert!(loaded.plugins_enabled.is_empty());
        assert_eq!(
            loaded.plugin_context_token_budget,
            DEFAULT_PLUGIN_CONTEXT_TOKEN_BUDGET
        );
        assert!(loaded.stream_markdown_enabled);
    }

    #[test]
    fn test_settings_set_available_models() {
        let mut settings = Settings::default();
        settings.model = "old-model".to_string();

        let models = vec![
            "claude-3-haiku-20240307".to_string(),
            "claude-sonnet-4-20250514".to_string(),
            "claude-3-opus-20240229".to_string(),
        ];

        settings.set_available_models(models.clone());

        assert_eq!(settings.available_models, models);
        // Should select sonnet since old model wasn't in list
        assert!(settings.model.contains("sonnet"));
    }

    #[test]
    fn test_settings_set_available_models_keeps_current() {
        let mut settings = Settings::default();
        settings.model = "claude-3-opus-20240229".to_string();

        let models = vec![
            "claude-3-haiku-20240307".to_string(),
            "claude-sonnet-4-20250514".to_string(),
            "claude-3-opus-20240229".to_string(),
        ];

        settings.set_available_models(models);

        // Should keep opus since it's in the list
        assert_eq!(settings.model, "claude-3-opus-20240229");
    }

    #[test]
    fn test_settings_serialization() {
        let settings = Settings::default();
        let json = serde_json::to_string(&settings).unwrap();
        let parsed: Settings = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.model, settings.model);
        assert_eq!(parsed.max_tokens, settings.max_tokens);
        assert_eq!(parsed.theme, settings.theme);
    }

    #[test]
    fn test_settings_model_display_name() {
        let mut settings = Settings::default();
        settings.model = "claude-code-claude-sonnet-4-20250514".to_string();
        assert_eq!(settings.model_display_name(), "Claude 4 Sonnet");
    }
}
