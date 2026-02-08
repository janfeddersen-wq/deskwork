//! Plugin compatibility layer.
//!
//! This module is retained only for the MCP server connection infrastructure
//! used by the executor. The plugin registry, loader, bundled assets, context
//! builder, runtime, and slash commands have been replaced by the
//! `skills::categories` system.

pub mod mcp_manager;
pub mod mcp_tool;
pub mod types;

pub use mcp_manager::{NamespacedMcpTool, PluginMcpManager};
pub use mcp_tool::PluginMcpTool;
