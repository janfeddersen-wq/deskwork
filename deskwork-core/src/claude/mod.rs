//! Claude model integration.
//!
//! Thin wrapper around serdes-ai-models for Claude Code OAuth.

mod model;

pub use model::{create_model, create_model_with_thinking};
