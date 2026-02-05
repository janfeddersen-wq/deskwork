//! Tool implementations for Deskwork.
//!
//! Provides serdesAI-compatible tools for file operations, shell commands, and more.

pub mod common;
pub mod diff;
pub mod file_ops;

// Tool implementations
pub mod delete_file_tool;
pub mod edit_file_tool;
pub mod grep_tool;
pub mod list_files_tool;
pub mod read_file_tool;
pub mod shell_tool;

// Registry
pub mod registry;

// Re-exports - file operations
pub use file_ops::{FileEntry, FileError, GrepMatch, GrepResult, ListFilesResult, ReadFileResult};

// Re-exports - tools
pub use delete_file_tool::DeleteFileTool;
pub use edit_file_tool::EditFileTool;
pub use grep_tool::GrepTool;
pub use list_files_tool::ListFilesTool;
pub use read_file_tool::ReadFileTool;
pub use shell_tool::RunShellCommandTool;

// Re-exports - registry
pub use registry::ToolRegistry;
