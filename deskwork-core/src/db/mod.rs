//! SQLite database layer for Deskwork.
//!
//! Provides persistent storage for:
//! - API keys (encrypted provider credentials)
//! - Settings (app preferences)
//! - Conversations and messages (chat history)

mod migrations;

use rusqlite::Connection;
use std::path::PathBuf;

/// Database connection wrapper.
///
/// Provides a high-level API for interacting with the SQLite database.
/// Automatically handles connection setup, migrations, and file permissions.
pub struct Database {
    conn: Connection,
    path: PathBuf,
}

impl Database {
    /// Open the database at the default location.
    ///
    /// Default path: `~/.local/share/deskwork/deskwork.db`
    pub fn open() -> anyhow::Result<Self> {
        let path = Self::default_path()?;
        Self::open_at(path)
    }

    /// Open the database at a specific path.
    ///
    /// Creates parent directories if they don't exist.
    /// Sets file permissions to 0600 on Unix (contains sensitive data!).
    pub fn open_at(path: PathBuf) -> anyhow::Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(&path)?;

        // Set restrictive file permissions (0600) on Unix systems.
        // The database contains sensitive data like API keys!
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Err(e) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            {
                tracing::warn!(path = %path.display(), error = %e, "Failed to set database file permissions");
            }
        }

        // Enable foreign keys for referential integrity
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;

        Ok(Self { conn, path })
    }

    /// Get the default database path.
    ///
    /// Returns `~/.local/share/deskwork/deskwork.db` (or platform equivalent).
    pub fn default_path() -> anyhow::Result<PathBuf> {
        let data_dir = dirs::data_dir()
            .or_else(|| dirs::home_dir().map(|h| h.join(".local/share")))
            .ok_or_else(|| anyhow::anyhow!("Could not determine data directory"))?;

        Ok(data_dir.join("deskwork").join("deskwork.db"))
    }

    /// Run database migrations.
    ///
    /// Safe to call multiple times - migrations are tracked and only run once.
    pub fn migrate(&self) -> anyhow::Result<()> {
        migrations::run_migrations(&self.conn)?;
        Ok(())
    }

    /// Get a reference to the underlying connection.
    ///
    /// Use sparingly - prefer the high-level methods when possible.
    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    /// Get the database file path.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    // =========================================================================
    // API Key Storage
    // =========================================================================

    /// Save an API key to the database (upsert).
    ///
    /// Use `ANTHROPIC_API_KEY` for Claude, `OPENAI_API_KEY` for OpenAI, etc.
    pub fn save_api_key(&self, name: &str, api_key: &str) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO api_keys (name, api_key, updated_at) VALUES (?, ?, unixepoch())
             ON CONFLICT(name) DO UPDATE SET api_key = excluded.api_key, updated_at = excluded.updated_at",
            [name, api_key],
        )?;
        Ok(())
    }

    /// Get an API key from the database.
    ///
    /// Returns `None` if the key doesn't exist.
    pub fn get_api_key(&self, name: &str) -> Result<Option<String>, rusqlite::Error> {
        let mut stmt = self
            .conn
            .prepare("SELECT api_key FROM api_keys WHERE name = ?")?;
        let result = stmt.query_row([name], |row| row.get(0));
        match result {
            Ok(key) => Ok(Some(key)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Check if an API key exists in the database.
    pub fn has_api_key(&self, name: &str) -> bool {
        self.get_api_key(name).ok().flatten().is_some()
    }

    /// Delete an API key from the database.
    ///
    /// No-op if the key doesn't exist.
    pub fn delete_api_key(&self, name: &str) -> Result<(), rusqlite::Error> {
        self.conn
            .execute("DELETE FROM api_keys WHERE name = ?", [name])?;
        Ok(())
    }

    /// List all stored API key names.
    pub fn list_api_keys(&self) -> Result<Vec<String>, rusqlite::Error> {
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM api_keys ORDER BY name")?;
        let rows = stmt.query_map([], |row| row.get(0))?;
        rows.collect()
    }

    // =========================================================================
    // Settings Storage
    // =========================================================================

    /// Save a setting to the database (upsert).
    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, unixepoch())
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            [key, value],
        )?;
        Ok(())
    }

    /// Get a setting from the database.
    ///
    /// Returns `None` if the setting doesn't exist.
    pub fn get_setting(&self, key: &str) -> Result<Option<String>, rusqlite::Error> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM settings WHERE key = ?")?;
        let result = stmt.query_row([key], |row| row.get(0));
        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Get a setting with a default value.
    ///
    /// Returns the default if the setting doesn't exist or on error.
    pub fn get_setting_or(&self, key: &str, default: &str) -> String {
        self.get_setting(key)
            .ok()
            .flatten()
            .unwrap_or_else(|| default.to_string())
    }

    /// Delete a setting from the database.
    pub fn delete_setting(&self, key: &str) -> Result<(), rusqlite::Error> {
        self.conn
            .execute("DELETE FROM settings WHERE key = ?", [key])?;
        Ok(())
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // -------------------------------------------------------------------------
    // Test Helpers
    // -------------------------------------------------------------------------

    fn setup_test_db() -> (TempDir, Database) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let db = Database::open_at(db_path).unwrap();
        db.migrate().unwrap();
        (temp_dir, db)
    }

    // -------------------------------------------------------------------------
    // Database Opening/Creation Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_open_and_migrate() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.db");
        let db = Database::open_at(path).unwrap();
        db.migrate().unwrap();
    }

    #[test]
    fn test_open_at_creates_parent_directories() {
        let tmp = TempDir::new().unwrap();
        let nested_path = tmp
            .path()
            .join("deep")
            .join("nested")
            .join("dir")
            .join("test.db");

        assert!(!nested_path.parent().unwrap().exists());

        let _db = Database::open_at(nested_path.clone()).unwrap();

        assert!(nested_path.exists());
    }

    #[test]
    fn test_open_at_reuses_existing_database() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.db");

        // First open - create and populate
        {
            let db = Database::open_at(path.clone()).unwrap();
            db.migrate().unwrap();
            db.save_api_key("TEST_KEY", "secret123").unwrap();
        }

        // Second open - should see existing data
        {
            let db = Database::open_at(path).unwrap();
            let key = db.get_api_key("TEST_KEY").unwrap();
            assert_eq!(key, Some("secret123".to_string()));
        }
    }

    #[test]
    fn test_default_path_returns_valid_path() {
        if let Ok(path) = Database::default_path() {
            assert!(path.ends_with("deskwork/deskwork.db"));
            assert!(path.parent().is_some());
        }
    }

    #[cfg(unix)]
    #[test]
    fn test_open_at_sets_restrictive_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("secure.db");

        let _db = Database::open_at(path.clone()).unwrap();

        let metadata = std::fs::metadata(&path).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "Database should have 0600 permissions");
    }

    #[test]
    fn test_foreign_keys_enabled() {
        let (_temp, db) = setup_test_db();

        let fk_status: i32 = db
            .conn()
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(fk_status, 1, "Foreign keys should be enabled");
    }

    // -------------------------------------------------------------------------
    // API Key Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_save_api_key_inserts_new_key() {
        let (_temp, db) = setup_test_db();

        db.save_api_key("ANTHROPIC_API_KEY", "sk-ant-test123")
            .unwrap();

        let key = db.get_api_key("ANTHROPIC_API_KEY").unwrap();
        assert_eq!(key, Some("sk-ant-test123".to_string()));
    }

    #[test]
    fn test_save_api_key_upserts_existing_key() {
        let (_temp, db) = setup_test_db();

        db.save_api_key("ANTHROPIC_API_KEY", "old_value").unwrap();
        db.save_api_key("ANTHROPIC_API_KEY", "new_value").unwrap();

        let key = db.get_api_key("ANTHROPIC_API_KEY").unwrap();
        assert_eq!(key, Some("new_value".to_string()));
    }

    #[test]
    fn test_save_api_key_multiple_providers() {
        let (_temp, db) = setup_test_db();

        db.save_api_key("OPENAI_API_KEY", "openai-key").unwrap();
        db.save_api_key("ANTHROPIC_API_KEY", "anthropic-key")
            .unwrap();

        assert_eq!(
            db.get_api_key("OPENAI_API_KEY").unwrap(),
            Some("openai-key".to_string())
        );
        assert_eq!(
            db.get_api_key("ANTHROPIC_API_KEY").unwrap(),
            Some("anthropic-key".to_string())
        );
    }

    #[test]
    fn test_get_api_key_returns_none_for_missing() {
        let (_temp, db) = setup_test_db();

        let key = db.get_api_key("NONEXISTENT_KEY").unwrap();
        assert!(key.is_none());
    }

    #[test]
    fn test_has_api_key_returns_true_when_exists() {
        let (_temp, db) = setup_test_db();

        db.save_api_key("TEST_KEY", "value").unwrap();
        assert!(db.has_api_key("TEST_KEY"));
    }

    #[test]
    fn test_has_api_key_returns_false_when_missing() {
        let (_temp, db) = setup_test_db();

        assert!(!db.has_api_key("NONEXISTENT_KEY"));
    }

    #[test]
    fn test_delete_api_key_removes_key() {
        let (_temp, db) = setup_test_db();

        db.save_api_key("DELETE_ME", "value").unwrap();
        assert!(db.has_api_key("DELETE_ME"));

        db.delete_api_key("DELETE_ME").unwrap();
        assert!(!db.has_api_key("DELETE_ME"));
    }

    #[test]
    fn test_delete_api_key_nonexistent_succeeds() {
        let (_temp, db) = setup_test_db();

        // Should not error when deleting non-existent key
        db.delete_api_key("NEVER_EXISTED").unwrap();
    }

    #[test]
    fn test_list_api_keys_empty() {
        let (_temp, db) = setup_test_db();

        let keys = db.list_api_keys().unwrap();
        assert!(keys.is_empty());
    }

    #[test]
    fn test_list_api_keys_returns_sorted() {
        let (_temp, db) = setup_test_db();

        db.save_api_key("ZEBRA_KEY", "z").unwrap();
        db.save_api_key("ALPHA_KEY", "a").unwrap();

        let keys = db.list_api_keys().unwrap();
        assert_eq!(keys, vec!["ALPHA_KEY", "ZEBRA_KEY"]);
    }

    // -------------------------------------------------------------------------
    // Settings Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_set_setting_inserts_new() {
        let (_temp, db) = setup_test_db();

        db.set_setting("theme", "dark").unwrap();

        let value = db.get_setting("theme").unwrap();
        assert_eq!(value, Some("dark".to_string()));
    }

    #[test]
    fn test_set_setting_upserts_existing() {
        let (_temp, db) = setup_test_db();

        db.set_setting("theme", "light").unwrap();
        db.set_setting("theme", "dark").unwrap();

        let value = db.get_setting("theme").unwrap();
        assert_eq!(value, Some("dark".to_string()));
    }

    #[test]
    fn test_get_setting_returns_none_for_missing() {
        let (_temp, db) = setup_test_db();

        let value = db.get_setting("nonexistent").unwrap();
        assert!(value.is_none());
    }

    #[test]
    fn test_get_setting_or_returns_value_when_exists() {
        let (_temp, db) = setup_test_db();

        db.set_setting("theme", "dark").unwrap();

        let value = db.get_setting_or("theme", "light");
        assert_eq!(value, "dark");
    }

    #[test]
    fn test_get_setting_or_returns_default_when_missing() {
        let (_temp, db) = setup_test_db();

        let value = db.get_setting_or("nonexistent", "default_value");
        assert_eq!(value, "default_value");
    }

    #[test]
    fn test_delete_setting_removes_setting() {
        let (_temp, db) = setup_test_db();

        db.set_setting("theme", "dark").unwrap();
        assert!(db.get_setting("theme").unwrap().is_some());

        db.delete_setting("theme").unwrap();
        assert!(db.get_setting("theme").unwrap().is_none());
    }

    // -------------------------------------------------------------------------
    // Edge Cases
    // -------------------------------------------------------------------------

    #[test]
    fn test_api_key_with_special_characters() {
        let (_temp, db) = setup_test_db();

        let special_key = "sk-ant_123!@#$%^&*()_+-=[]{}|;':\",./<>?";
        db.save_api_key("SPECIAL_KEY", special_key).unwrap();

        let retrieved = db.get_api_key("SPECIAL_KEY").unwrap();
        assert_eq!(retrieved, Some(special_key.to_string()));
    }

    #[test]
    fn test_setting_with_json_value() {
        let (_temp, db) = setup_test_db();

        let json_value = r#"{"font_size": 14, "show_line_numbers": true}"#;
        db.set_setting("editor_config", json_value).unwrap();

        let retrieved = db.get_setting("editor_config").unwrap();
        assert_eq!(retrieved, Some(json_value.to_string()));
    }
}
