//! Model registry for loading and managing model configurations.

use std::collections::HashMap;

use rusqlite::params;
use tracing::debug;

use crate::db::Database;

use super::config::ModelConfig;
use super::types::{ModelConfigError, ModelType};

/// Registry of available models loaded from the database.
#[derive(Debug, Default)]
pub struct ModelRegistry {
    models: HashMap<String, ModelConfig>,
}

impl ModelRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Load models from the database.
    pub fn load_from_db(db: &Database) -> Result<Self, ModelConfigError> {
        let mut registry = Self::new();

        let mut stmt = db
            .conn()
            .prepare(
                "SELECT name, model_type, model_id, context_length, supports_thinking,
                        supports_vision, supports_tools, description
                 FROM models ORDER BY name",
            )
            .map_err(|e| ModelConfigError::Database(e.to_string()))?;

        let rows = stmt
            .query_map([], |row| {
                let model_type_str: String = row.get(1)?;

                Ok(ModelConfig {
                    name: row.get(0)?,
                    model_type: ModelType::parse_lossy(&model_type_str),
                    model_id: row.get(2)?,
                    context_length: row.get::<_, i64>(3)? as usize,
                    supports_thinking: row.get::<_, i64>(4)? != 0,
                    supports_vision: row.get::<_, i64>(5)? != 0,
                    supports_tools: row.get::<_, i64>(6)? != 0,
                    description: row.get(7)?,
                })
            })
            .map_err(|e| ModelConfigError::Database(e.to_string()))?;

        for config in rows.flatten() {
            debug!(
                model = %config.name,
                model_type = %config.model_type,
                "Loaded model from database"
            );
            registry.models.insert(config.name.clone(), config);
        }

        debug!(
            total_models = registry.models.len(),
            "ModelRegistry loaded from database"
        );

        Ok(registry)
    }

    /// Add a model to the database.
    pub fn add_model_to_db(db: &Database, config: &ModelConfig) -> Result<(), ModelConfigError> {
        Self::add_model_to_db_with_source(db, config, "oauth")
    }

    /// Add a model to the database with explicit source tracking.
    pub fn add_model_to_db_with_source(
        db: &Database,
        config: &ModelConfig,
        source: &str,
    ) -> Result<(), ModelConfigError> {
        debug!(
            model = %config.name,
            model_type = %config.model_type,
            source = %source,
            "Saving model to database"
        );

        db.conn()
            .execute(
                "INSERT OR REPLACE INTO models (name, model_type, model_id, context_length,
                    supports_thinking, supports_vision, supports_tools, description,
                    source, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, unixepoch())",
                params![
                    &config.name,
                    config.model_type.to_string(),
                    &config.model_id,
                    config.context_length as i64,
                    config.supports_thinking as i64,
                    config.supports_vision as i64,
                    config.supports_tools as i64,
                    &config.description,
                    source,
                ],
            )
            .map_err(|e| ModelConfigError::Database(e.to_string()))?;

        Ok(())
    }

    /// Remove a model from the database.
    pub fn remove_model_from_db(db: &Database, name: &str) -> Result<(), ModelConfigError> {
        db.conn()
            .execute("DELETE FROM models WHERE name = ?", params![name])
            .map_err(|e| ModelConfigError::Database(e.to_string()))?;
        Ok(())
    }

    /// Reload the registry from database.
    pub fn reload_from_db(&mut self, db: &Database) -> Result<(), ModelConfigError> {
        self.models.clear();
        let fresh = Self::load_from_db(db)?;
        self.models = fresh.models;
        Ok(())
    }

    /// Add a model to the registry (in memory).
    pub fn add(&mut self, config: ModelConfig) {
        self.models.insert(config.name.clone(), config);
    }

    /// Get a model by name.
    pub fn get(&self, name: &str) -> Option<&ModelConfig> {
        self.models.get(name)
    }

    /// Check if a model exists.
    pub fn contains(&self, name: &str) -> bool {
        self.models.contains_key(name)
    }

    /// Get all model names as a sorted vector.
    pub fn list(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.models.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }

    /// Get all models.
    pub fn all(&self) -> impl Iterator<Item = &ModelConfig> {
        self.models.values()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    /// Number of models in the registry.
    pub fn len(&self) -> usize {
        self.models.len()
    }

    /// List Claude Code models (sorted).
    pub fn list_claude_code_models(&self) -> Vec<String> {
        let mut models: Vec<String> = self
            .models
            .iter()
            .filter(|(_, config)| matches!(config.model_type, ModelType::ClaudeCode))
            .map(|(name, _)| name.clone())
            .collect();
        models.sort();
        models
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_test_db() -> (TempDir, Database) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Database::open_at(db_path).unwrap();
        db.migrate().unwrap(); // Run migrations to create tables
        (temp_dir, db)
    }

    fn create_test_model(name: &str) -> ModelConfig {
        ModelConfig {
            name: name.to_string(),
            model_type: ModelType::ClaudeCode,
            model_id: Some(
                name.strip_prefix("claude-code-")
                    .unwrap_or(name)
                    .to_string(),
            ),
            context_length: 200_000,
            supports_thinking: true,
            supports_vision: true,
            supports_tools: true,
            description: None,
        }
    }

    #[test]
    fn test_registry_new() {
        let registry = ModelRegistry::new();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_registry_add() {
        let mut registry = ModelRegistry::new();
        let config = create_test_model("claude-code-claude-sonnet-4-20250514");
        registry.add(config);

        assert_eq!(registry.len(), 1);
        assert!(registry.contains("claude-code-claude-sonnet-4-20250514"));
    }

    #[test]
    fn test_registry_get() {
        let mut registry = ModelRegistry::new();
        let config = create_test_model("claude-code-claude-sonnet-4-20250514");
        registry.add(config);

        let model = registry
            .get("claude-code-claude-sonnet-4-20250514")
            .unwrap();
        assert_eq!(model.name, "claude-code-claude-sonnet-4-20250514");
        assert!(model.supports_thinking);
    }

    #[test]
    fn test_registry_list() {
        let mut registry = ModelRegistry::new();
        registry.add(create_test_model("claude-code-claude-sonnet-4-20250514"));
        registry.add(create_test_model("claude-code-claude-haiku-3-5-20241022"));

        let names = registry.list();
        assert_eq!(names.len(), 2);
        // Should be sorted
        assert_eq!(names[0], "claude-code-claude-haiku-3-5-20241022");
        assert_eq!(names[1], "claude-code-claude-sonnet-4-20250514");
    }

    #[test]
    fn test_registry_db_roundtrip() {
        let (_temp, db) = setup_test_db();

        let config = create_test_model("claude-code-claude-sonnet-4-20250514");
        ModelRegistry::add_model_to_db(&db, &config).unwrap();

        let registry = ModelRegistry::load_from_db(&db).unwrap();
        assert_eq!(registry.len(), 1);

        let loaded = registry
            .get("claude-code-claude-sonnet-4-20250514")
            .unwrap();
        assert_eq!(loaded.name, config.name);
        assert_eq!(loaded.supports_thinking, config.supports_thinking);
    }

    #[test]
    fn test_registry_remove_from_db() {
        let (_temp, db) = setup_test_db();

        let config = create_test_model("claude-code-claude-sonnet-4-20250514");
        ModelRegistry::add_model_to_db(&db, &config).unwrap();

        ModelRegistry::remove_model_from_db(&db, "claude-code-claude-sonnet-4-20250514").unwrap();

        let registry = ModelRegistry::load_from_db(&db).unwrap();
        assert!(registry.is_empty());
    }

    #[test]
    fn test_registry_reload() {
        let (_temp, db) = setup_test_db();
        let mut registry = ModelRegistry::new();

        // Add model to DB
        let config = create_test_model("claude-code-claude-sonnet-4-20250514");
        ModelRegistry::add_model_to_db(&db, &config).unwrap();

        // Registry is empty
        assert!(registry.is_empty());

        // Reload
        registry.reload_from_db(&db).unwrap();
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_list_claude_code_models() {
        let mut registry = ModelRegistry::new();
        registry.add(create_test_model("claude-code-claude-sonnet-4-20250514"));
        registry.add(create_test_model("claude-code-claude-haiku-3-5-20241022"));

        let models = registry.list_claude_code_models();
        assert_eq!(models.len(), 2);
    }
}
