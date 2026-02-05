//! DeleteFile tool implementation.
//!
//! Provides a serdesAI-compatible tool for deleting files.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use tracing::debug;

use serdes_ai_tools::{RunContext, SchemaBuilder, Tool, ToolDefinition, ToolResult, ToolReturn};

/// Tool for deleting files.
#[derive(Debug, Clone, Default)]
pub struct DeleteFileTool;

#[derive(Debug, Deserialize)]
struct DeleteFileArgs {
    file_path: String,
}

#[async_trait]
impl Tool for DeleteFileTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "delete_file",
            "Safely delete a file. Will fail if the path is a directory.",
        )
        .with_parameters(
            SchemaBuilder::new()
                .string("file_path", "Path to the file to delete.", true)
                .build()
                .expect("schema build failed"),
        )
    }

    async fn call(&self, _ctx: &RunContext, args: JsonValue) -> ToolResult {
        debug!(tool = "delete_file", ?args, "Tool called");

        let args: DeleteFileArgs = super::common::parse_tool_args_lenient(
            "delete_file",
            args.clone(),
            self.definition().parameters(),
        )?;

        let path = std::path::Path::new(&args.file_path);

        if !path.exists() {
            return Ok(ToolReturn::error(format!(
                "File not found: {}",
                args.file_path
            )));
        }

        if path.is_dir() {
            return Ok(ToolReturn::error(format!(
                "Cannot delete directory with this tool: {}",
                args.file_path
            )));
        }

        match std::fs::remove_file(path) {
            Ok(()) => Ok(ToolReturn::text(format!(
                "Successfully deleted: {}",
                args.file_path
            ))),
            Err(e) => Ok(ToolReturn::error(format!("Failed to delete file: {}", e))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_delete_file_success() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("to_delete.txt");
        fs::write(&file_path, "content").unwrap();
        assert!(file_path.exists());

        let tool = DeleteFileTool;
        let ctx = RunContext::minimal("test");

        let result = tool
            .call(
                &ctx,
                serde_json::json!({ "file_path": file_path.to_str().unwrap() }),
            )
            .await;

        assert!(result.is_ok());
        let ret = result.unwrap();
        assert!(!ret.is_error());
        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_delete_file_not_found() {
        let tool = DeleteFileTool;
        let ctx = RunContext::minimal("test");

        let result = tool
            .call(
                &ctx,
                serde_json::json!({ "file_path": "/nonexistent/path/file.txt" }),
            )
            .await;

        assert!(result.is_ok());
        let ret = result.unwrap();
        assert!(ret.is_error());
        assert!(ret.as_text().unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn test_delete_file_is_directory() {
        let dir = TempDir::new().unwrap();
        let subdir = dir.path().join("subdir");
        fs::create_dir(&subdir).unwrap();

        let tool = DeleteFileTool;
        let ctx = RunContext::minimal("test");

        let result = tool
            .call(
                &ctx,
                serde_json::json!({ "file_path": subdir.to_str().unwrap() }),
            )
            .await;

        assert!(result.is_ok());
        let ret = result.unwrap();
        assert!(ret.is_error());
        assert!(ret.as_text().unwrap().contains("Cannot delete directory"));
    }
}
