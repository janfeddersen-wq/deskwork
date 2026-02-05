//! OAuth token storage in SQLite.

use crate::db::Database;
use chrono::Utc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum TokenStorageError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Provider not authenticated: {0}")]
    NotAuthenticated(String),
    #[error("Token expired")]
    Expired,
}

/// Stored OAuth tokens.
#[derive(Debug, Clone)]
pub struct StoredTokens {
    pub provider: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
    pub account_id: Option<String>,
    pub extra_data: Option<String>,
    pub updated_at: i64,
}

impl StoredTokens {
    /// Check if the token is expired.
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now().timestamp() >= expires_at
        } else {
            false
        }
    }

    /// Check if the token will expire within the given seconds.
    pub fn expires_within(&self, seconds: i64) -> bool {
        if let Some(expires_at) = self.expires_at {
            Utc::now().timestamp() >= expires_at - seconds
        } else {
            false
        }
    }
}

/// Token storage operations.
pub struct TokenStorage<'a> {
    db: &'a Database,
}

impl<'a> TokenStorage<'a> {
    /// Create a new token storage.
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// Save tokens for a provider.
    pub fn save(
        &self,
        provider: &str,
        access_token: &str,
        refresh_token: Option<&str>,
        expires_in: Option<u64>,
        account_id: Option<&str>,
        extra_data: Option<&str>,
    ) -> Result<(), TokenStorageError> {
        let expires_at = expires_in.map(|secs| Utc::now().timestamp() + secs as i64);

        self.db.conn().execute(
            "INSERT INTO oauth_tokens (provider, access_token, refresh_token, expires_at, account_id, extra_data, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, unixepoch())
             ON CONFLICT(provider) DO UPDATE SET 
                access_token = excluded.access_token,
                refresh_token = COALESCE(excluded.refresh_token, oauth_tokens.refresh_token),
                expires_at = excluded.expires_at,
                account_id = COALESCE(excluded.account_id, oauth_tokens.account_id),
                extra_data = COALESCE(excluded.extra_data, oauth_tokens.extra_data),
                updated_at = excluded.updated_at",
            rusqlite::params![
                provider,
                access_token,
                refresh_token,
                expires_at,
                account_id,
                extra_data,
            ],
        )?;

        Ok(())
    }

    /// Load tokens for a provider.
    pub fn load(&self, provider: &str) -> Result<Option<StoredTokens>, TokenStorageError> {
        let result = self.db.conn().query_row(
            "SELECT provider, access_token, refresh_token, expires_at, account_id, extra_data, updated_at
             FROM oauth_tokens WHERE provider = ?",
            [provider],
            |row| {
                Ok(StoredTokens {
                    provider: row.get(0)?,
                    access_token: row.get(1)?,
                    refresh_token: row.get(2)?,
                    expires_at: row.get(3)?,
                    account_id: row.get(4)?,
                    extra_data: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            },
        );

        match result {
            Ok(tokens) => Ok(Some(tokens)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(TokenStorageError::Database(e)),
        }
    }

    /// Delete tokens for a provider.
    pub fn delete(&self, provider: &str) -> Result<(), TokenStorageError> {
        self.db
            .conn()
            .execute("DELETE FROM oauth_tokens WHERE provider = ?", [provider])?;
        Ok(())
    }

    /// Check if a provider is authenticated (has tokens).
    pub fn is_authenticated(&self, provider: &str) -> Result<bool, TokenStorageError> {
        Ok(self.load(provider)?.is_some())
    }

    /// List all authenticated providers.
    pub fn list_providers(&self) -> Result<Vec<String>, TokenStorageError> {
        let mut stmt = self
            .db
            .conn()
            .prepare("SELECT provider FROM oauth_tokens ORDER BY provider")?;

        let rows = stmt.query_map([], |row| row.get(0))?;
        let mut providers = Vec::new();
        for row in rows {
            providers.push(row?);
        }
        Ok(providers)
    }
}

/// Check if we have valid OAuth tokens for a provider.
pub fn has_oauth_tokens(db: &Database, provider: &str) -> bool {
    TokenStorage::new(db)
        .load(provider)
        .map(|t| t.is_some_and(|tokens| !tokens.is_expired()))
        .unwrap_or(false)
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

    #[test]
    fn test_stored_tokens_not_expired() {
        let tokens = StoredTokens {
            provider: "test".to_string(),
            access_token: "token".to_string(),
            refresh_token: None,
            expires_at: Some(Utc::now().timestamp() + 3600),
            account_id: None,
            extra_data: None,
            updated_at: Utc::now().timestamp(),
        };
        assert!(!tokens.is_expired());
    }

    #[test]
    fn test_stored_tokens_is_expired() {
        let tokens = StoredTokens {
            provider: "test".to_string(),
            access_token: "token".to_string(),
            refresh_token: None,
            expires_at: Some(Utc::now().timestamp() - 3600),
            account_id: None,
            extra_data: None,
            updated_at: Utc::now().timestamp(),
        };
        assert!(tokens.is_expired());
    }

    #[test]
    fn test_stored_tokens_expires_within() {
        let tokens = StoredTokens {
            provider: "test".to_string(),
            access_token: "token".to_string(),
            refresh_token: None,
            expires_at: Some(Utc::now().timestamp() + 60),
            account_id: None,
            extra_data: None,
            updated_at: Utc::now().timestamp(),
        };
        assert!(tokens.expires_within(120));
        assert!(!tokens.expires_within(30));
    }

    #[test]
    fn test_save_and_load_tokens() {
        let (_temp, db) = setup_test_db();
        let storage = TokenStorage::new(&db);

        storage
            .save(
                "provider1",
                "access123",
                Some("refresh456"),
                Some(3600),
                None,
                None,
            )
            .unwrap();

        let loaded = storage.load("provider1").unwrap().unwrap();
        assert_eq!(loaded.access_token, "access123");
        assert_eq!(loaded.refresh_token, Some("refresh456".to_string()));
    }

    #[test]
    fn test_load_nonexistent_provider() {
        let (_temp, db) = setup_test_db();
        let storage = TokenStorage::new(&db);

        let result = storage.load("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_delete_tokens() {
        let (_temp, db) = setup_test_db();
        let storage = TokenStorage::new(&db);

        storage
            .save("provider1", "token", None, None, None, None)
            .unwrap();
        storage.delete("provider1").unwrap();

        assert!(storage.load("provider1").unwrap().is_none());
    }

    #[test]
    fn test_is_authenticated() {
        let (_temp, db) = setup_test_db();
        let storage = TokenStorage::new(&db);

        assert!(!storage.is_authenticated("provider1").unwrap());

        storage
            .save("provider1", "token", None, None, None, None)
            .unwrap();
        assert!(storage.is_authenticated("provider1").unwrap());
    }

    #[test]
    fn test_list_providers() {
        let (_temp, db) = setup_test_db();
        let storage = TokenStorage::new(&db);

        storage
            .save("zebra", "token1", None, None, None, None)
            .unwrap();
        storage
            .save("alpha", "token2", None, None, None, None)
            .unwrap();

        let providers = storage.list_providers().unwrap();
        assert_eq!(providers, vec!["alpha", "zebra"]);
    }

    #[test]
    fn test_save_preserves_refresh_token_on_update() {
        let (_temp, db) = setup_test_db();
        let storage = TokenStorage::new(&db);

        storage
            .save("provider1", "access1", Some("refresh1"), None, None, None)
            .unwrap();

        storage
            .save("provider1", "access2", None, None, None, None)
            .unwrap();

        let loaded = storage.load("provider1").unwrap().unwrap();
        assert_eq!(loaded.access_token, "access2");
        assert_eq!(loaded.refresh_token, Some("refresh1".to_string()));
    }

    #[test]
    fn test_has_oauth_tokens() {
        let (_temp, db) = setup_test_db();
        let storage = TokenStorage::new(&db);

        assert!(!has_oauth_tokens(&db, "provider1"));

        storage
            .save("provider1", "token", None, Some(3600), None, None)
            .unwrap();
        assert!(has_oauth_tokens(&db, "provider1"));
    }

    #[test]
    fn test_has_oauth_tokens_expired() {
        let (_temp, db) = setup_test_db();

        // Manually insert an expired token
        db.conn()
            .execute(
                "INSERT INTO oauth_tokens (provider, access_token, expires_at, updated_at)
                 VALUES ('expired', 'token', ?, unixepoch())",
                [Utc::now().timestamp() - 3600],
            )
            .unwrap();

        assert!(!has_oauth_tokens(&db, "expired"));
    }
}
