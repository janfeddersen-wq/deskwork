//! Model configuration and registry.
//!
//! This module provides:
//! - `ModelType` - Supported AI provider types
//! - `ModelConfig` - Per-model configuration
//! - `ModelRegistry` - Database-backed model storage

mod config;
mod registry;
mod types;

pub use config::ModelConfig;
pub use registry::ModelRegistry;
pub use types::{ModelConfigError, ModelType};
