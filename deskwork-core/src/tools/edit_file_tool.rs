//! EditFile tool implementation.
//!
//! Provides a serdesAI-compatible tool for creating or editing files.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use tracing::debug;

use serdes_ai_tools::{RunContext, SchemaBuilder, Tool, ToolDefinition, ToolResult, ToolReturn};

use super::{diff, file_ops};

/// Tool for creating or editing files.
#[derive(Debug, Clone, Default)]
pub struct EditFileTool;

#[derive(Debug, Deserialize)]
struct EditFileArgs {
    file_path: String,
    content: Option<String>,
    diff: Option<String>,
    #[serde(default)]
    create_directories: bool,
}

#[async_trait]
impl Tool for EditFileTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "edit_file",
            "Create or edit a file. Can either write full content or apply a unified diff patch. \
             For new files, provide content. For modifications, you can provide either new content \
             or a unified diff.",
        )
        .with_parameters(
            SchemaBuilder::new()
                .string("file_path", "Path to the file to create or edit.", true)
                .string(
                    "content",
                    "The full content to write to the file. Use this for new files or full replacements.",
                    false,
                )
                .string(
                    "diff",
                    "A unified diff to apply to the existing file. Use this for targeted modifications.",
                    false,
                )
                .boolean(
                    "create_directories",
                    "Whether to create parent directories if they don't exist. Defaults to false.",
                    false,
                )
                .build()
                .expect("schema build failed"),
        )
    }

    async fn call(&self, _ctx: &RunContext, args: JsonValue) -> ToolResult {
        debug!(tool = "edit_file", ?args, "Tool called");

        let args: EditFileArgs = super::common::parse_tool_args_lenient(
            "edit_file",
            args.clone(),
            self.definition().parameters(),
        )?;

        // Determine what content to write
        let final_content = if let Some(diff_text) = &args.diff {
            // Apply diff to existing file
            let original = match file_ops::read_file(&args.file_path, None, None, None) {
                Ok(result) => result.content,
                Err(file_ops::FileError::NotFound(_)) => {
                    // For new files via diff, start with empty
                    String::new()
                }
                Err(e) => return Ok(ToolReturn::error(format!("Failed to read file: {}", e))),
            };

            match diff::apply_unified_diff(&original, diff_text) {
                Ok(content) => content,
                Err(e) => return Ok(ToolReturn::error(format!("Failed to apply diff: {}", e))),
            }
        } else if let Some(content) = args.content {
            content
        } else {
            return Ok(ToolReturn::error(
                "Either 'content' or 'diff' must be provided".to_string(),
            ));
        };

        // Write the file
        match file_ops::write_file(&args.file_path, &final_content, args.create_directories) {
            Ok(()) => {
                let line_count = final_content.lines().count();
                let byte_count = final_content.len();
                let action = if args.diff.is_some() {
                    "patched"
                } else {
                    "wrote"
                };
                Ok(ToolReturn::text(format!(
                    "Successfully {} {} lines ({} bytes) to {}",
                    action, line_count, byte_count, args.file_path
                )))
            }
            Err(e) => Ok(ToolReturn::error(format!("Failed to write file: {}", e))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_edit_file_creates_new_file() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("new.txt");

        let tool = EditFileTool;
        let ctx = RunContext::minimal("test");

        let result = tool
            .call(
                &ctx,
                serde_json::json!({
                    "file_path": file_path.to_str().unwrap(),
                    "content": "hello world"
                }),
            )
            .await;

        assert!(result.is_ok());
        assert!(file_path.exists());
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "hello world");
    }

    #[tokio::test]
    async fn test_edit_file_overwrites_existing() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("existing.txt");
        fs::write(&file_path, "old content").unwrap();

        let tool = EditFileTool;
        let ctx = RunContext::minimal("test");

        let result = tool
            .call(
                &ctx,
                serde_json::json!({
                    "file_path": file_path.to_str().unwrap(),
                    "content": "new content"
                }),
            )
            .await;

        assert!(result.is_ok());
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "new content");
    }

    #[tokio::test]
    async fn test_edit_file_applies_diff() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("patch.txt");
        fs::write(&file_path, "line 1\nline 2\nline 3").unwrap();

        let diff = r#"--- a/patch.txt
+++ b/patch.txt
@@ -1,3 +1,3 @@
 line 1
-line 2
+LINE 2 MODIFIED
 line 3
"#;

        let tool = EditFileTool;
        let ctx = RunContext::minimal("test");

        let result = tool
            .call(
                &ctx,
                serde_json::json!({
                    "file_path": file_path.to_str().unwrap(),
                    "diff": diff
                }),
            )
            .await;

        assert!(result.is_ok());
        let content = fs::read_to_string(&file_path).unwrap();
        assert!(content.contains("LINE 2 MODIFIED"));
    }

    #[tokio::test]
    async fn test_edit_file_creates_directories() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("sub/dir/new.txt");

        let tool = EditFileTool;
        let ctx = RunContext::minimal("test");

        let result = tool
            .call(
                &ctx,
                serde_json::json!({
                    "file_path": file_path.to_str().unwrap(),
                    "content": "nested content",
                    "create_directories": true
                }),
            )
            .await;

        assert!(result.is_ok());
        assert!(file_path.exists());
    }

    #[tokio::test]
    async fn test_edit_file_requires_content_or_diff() {
        let tool = EditFileTool;
        let ctx = RunContext::minimal("test");

        let result = tool
            .call(
                &ctx,
                serde_json::json!({
                    "file_path": "/tmp/test.txt"
                }),
            )
            .await;

        assert!(result.is_ok());
        let ret = result.unwrap();
        assert!(ret.is_error());
    }
}
