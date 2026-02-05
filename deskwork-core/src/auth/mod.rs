//! Authentication module for Claude OAuth flow.
//!
//! This module provides:
//! - Token storage in SQLite database
//! - Claude Code OAuth PKCE flow
//! - Automatic token refresh
//! - Model fetching from API

pub mod claude;
pub mod storage;

pub use claude::{
    fetch_claude_models, filter_latest_models, get_claude_code_model, run_claude_code_auth,
    save_claude_models_to_db, ClaudeCodeAuth, ClaudeCodeAuthError,
};
pub use storage::{has_oauth_tokens, StoredTokens, TokenStorage, TokenStorageError};
