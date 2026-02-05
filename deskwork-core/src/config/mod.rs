//! Configuration module for Deskwork.
//!
//! Manages application settings stored in SQLite.

mod settings;

pub use settings::{model_display_name, RenderMode, Settings, Theme, DEFAULT_MODEL};
