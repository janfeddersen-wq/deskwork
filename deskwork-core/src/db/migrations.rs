//! Database migrations for Deskwork.
//!
//! Simple migration system that tracks applied migrations and runs each only once.

use rusqlite::Connection;

/// SQL for the initial schema migration.
const MIGRATION_001_INITIAL: &str = r#"
-- API keys table (for storing provider keys like ANTHROPIC_API_KEY)
CREATE TABLE IF NOT EXISTS api_keys (
    name TEXT PRIMARY KEY,
    api_key TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);

-- Settings table (key-value store for app preferences)
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);

-- Conversations table (chat sessions)
CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY,
    title TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);

-- Messages table (individual messages in conversations)
CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system')),
    content TEXT NOT NULL,
    tool_calls TEXT,  -- JSON array of tool calls if any
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_messages_conversation ON messages(conversation_id);
"#;

/// SQL for OAuth tokens migration.
const MIGRATION_002_OAUTH_TOKENS: &str = r#"
-- OAuth tokens table (for storing OAuth tokens from providers)
CREATE TABLE IF NOT EXISTS oauth_tokens (
    provider TEXT PRIMARY KEY,
    access_token TEXT NOT NULL,
    refresh_token TEXT,
    expires_at INTEGER,
    account_id TEXT,
    extra_data TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);
"#;

/// SQL for models table migration.
const MIGRATION_003_MODELS: &str = r#"
-- Models table (for storing model configurations)
CREATE TABLE IF NOT EXISTS models (
    name TEXT PRIMARY KEY,
    model_type TEXT NOT NULL,
    model_id TEXT,
    context_length INTEGER NOT NULL DEFAULT 200000,
    supports_thinking INTEGER NOT NULL DEFAULT 0,
    supports_vision INTEGER NOT NULL DEFAULT 1,
    supports_tools INTEGER NOT NULL DEFAULT 1,
    description TEXT,
    source TEXT DEFAULT 'oauth',
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);
"#;

/// All migrations in order. Each is (name, sql).
const MIGRATIONS: &[(&str, &str)] = &[
    ("001_initial", MIGRATION_001_INITIAL),
    ("002_oauth_tokens", MIGRATION_002_OAUTH_TOKENS),
    ("003_models", MIGRATION_003_MODELS),
];

/// Run all pending migrations.
///
/// Creates the migrations tracking table if needed, then applies any migrations
/// that haven't been run yet.
pub fn run_migrations(conn: &Connection) -> anyhow::Result<()> {
    // Create migrations table if it doesn't exist
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS migrations (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            applied_at INTEGER NOT NULL DEFAULT (unixepoch())
        );",
    )?;

    for (name, sql) in MIGRATIONS {
        let applied: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM migrations WHERE name = ?)",
            [name],
            |row| row.get(0),
        )?;

        if !applied {
            tracing::info!(migration = %name, "Running migration");
            conn.execute_batch(sql)?;
            conn.execute("INSERT INTO migrations (name) VALUES (?)", [name])?;
            tracing::info!(migration = %name, "Migration complete");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn test_migrations_are_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();

        // Run migrations multiple times
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap();
        run_migrations(&conn).unwrap();

        // Should still work
        let count: i32 = conn
            .query_row("SELECT COUNT(*) FROM migrations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 3); // Three migrations applied
    }

    #[test]
    fn test_migrations_create_expected_tables() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        run_migrations(&conn).unwrap();

        // Query sqlite_master for tables
        let tables: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .unwrap();
            let rows = stmt.query_map([], |row| row.get(0)).unwrap();
            rows.map(|r| r.unwrap()).collect()
        };

        assert!(tables.contains(&"api_keys".to_string()));
        assert!(tables.contains(&"settings".to_string()));
        assert!(tables.contains(&"conversations".to_string()));
        assert!(tables.contains(&"messages".to_string()));
        assert!(tables.contains(&"migrations".to_string()));
        assert!(tables.contains(&"oauth_tokens".to_string()));
        assert!(tables.contains(&"models".to_string()));
    }
}
