//! ReadFile tool implementation.
//!
//! Provides a serdesAI-compatible tool for reading file contents.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use tracing::debug;

use serdes_ai_tools::{RunContext, SchemaBuilder, Tool, ToolDefinition, ToolResult, ToolReturn};

use super::file_ops::{self, FileError};

/// Tool for reading file contents.
#[derive(Debug, Clone, Default)]
pub struct ReadFileTool;

#[derive(Debug, Deserialize)]
struct ReadFileArgs {
    file_path: String,
    start_line: Option<usize>,
    num_lines: Option<usize>,
}

#[async_trait]
impl Tool for ReadFileTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "read_file",
            "Read file contents with optional line-range selection. \
             Protects against reading excessively large files that could \
             overwhelm the context window.",
        )
        .with_parameters(
            SchemaBuilder::new()
                .string(
                    "file_path",
                    "Path to the file to read. Can be relative or absolute.",
                    true,
                )
                .integer(
                    "start_line",
                    "Starting line number for partial reads (1-based indexing). \
                     If specified, num_lines should also be provided.",
                    false,
                )
                .integer(
                    "num_lines",
                    "Number of lines to read starting from start_line.",
                    false,
                )
                .build()
                .expect("schema build failed"),
        )
    }

    async fn call(&self, _ctx: &RunContext, args: JsonValue) -> ToolResult {
        debug!(tool = "read_file", ?args, "Tool called");

        let args: ReadFileArgs = super::common::parse_tool_args_lenient(
            "read_file",
            args.clone(),
            self.definition().parameters(),
        )?;

        match file_ops::read_file(&args.file_path, args.start_line, args.num_lines, None) {
            Ok(result) => {
                let output = format_read_file_output(&result, &args);
                Ok(ToolReturn::text(output))
            }
            Err(FileError::NotFound(path)) => {
                Ok(ToolReturn::error(format!("File not found: {}", path)))
            }
            Err(FileError::TooLarge(size, max)) => Ok(ToolReturn::error(format!(
                "File too large: {} bytes (max: {}). \
                 Use start_line and num_lines for partial reads.",
                size, max
            ))),
            Err(FileError::TokenLimitExceeded {
                estimated_tokens,
                total_lines,
                suggested_chunk_size,
            }) => Ok(ToolReturn::error(format!(
                "[FILE TOO LARGE: ~{} tokens, {} lines]\n\
                 This file exceeds the 10,000 token safety limit.\n\
                 Please read it in chunks using start_line and num_lines parameters.\n\
                 Suggested: start_line=1, num_lines={}",
                estimated_tokens, total_lines, suggested_chunk_size
            ))),
            Err(e) => Ok(ToolReturn::error(format!("Failed to read file: {}", e))),
        }
    }
}

/// Format read_file result with optional metadata header.
fn format_read_file_output(result: &file_ops::ReadFileResult, args: &ReadFileArgs) -> String {
    if args.start_line.is_some() {
        let start = args.start_line.unwrap_or(1);
        let end = start + args.num_lines.unwrap_or(result.lines) - 1;
        format!(
            "# File: {} (lines {}..{} of {})\n{}",
            result.path, start, end, result.lines, result.content
        )
    } else {
        result.content.clone()
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_read_file_tool_basic() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "Hello, World!").unwrap();

        let tool = ReadFileTool;
        let ctx = RunContext::minimal("test");

        let result = tool
            .call(
                &ctx,
                serde_json::json!({
                    "file_path": file_path.to_str().unwrap()
                }),
            )
            .await;

        assert!(result.is_ok());
        let output = result.unwrap().as_text().unwrap().to_string();
        assert_eq!(output, "Hello, World!");
    }

    #[tokio::test]
    async fn test_read_file_tool_with_line_range() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "Line 1\nLine 2\nLine 3\nLine 4\nLine 5").unwrap();

        let tool = ReadFileTool;
        let ctx = RunContext::minimal("test");

        let result = tool
            .call(
                &ctx,
                serde_json::json!({
                    "file_path": file_path.to_str().unwrap(),
                    "start_line": 2,
                    "num_lines": 2
                }),
            )
            .await;

        assert!(result.is_ok());
        let output = result.unwrap().as_text().unwrap().to_string();
        assert!(output.contains("Line 2"));
        assert!(output.contains("Line 3"));
        assert!(output.contains("lines 2..3 of 5"));
    }

    #[tokio::test]
    async fn test_read_file_tool_not_found() {
        let tool = ReadFileTool;
        let ctx = RunContext::minimal("test");

        let result = tool
            .call(
                &ctx,
                serde_json::json!({
                    "file_path": "/nonexistent/file.txt"
                }),
            )
            .await;

        assert!(result.is_ok());
        let output = result.unwrap().as_text().unwrap().to_string();
        assert!(output.contains("not found"));
    }

    #[tokio::test]
    async fn test_read_file_tool_token_limit() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("large.txt");
        let large_content = "x".repeat(50_000); // ~12,500 tokens
        fs::write(&file_path, &large_content).unwrap();

        let tool = ReadFileTool;
        let ctx = RunContext::minimal("test");

        let result = tool
            .call(
                &ctx,
                serde_json::json!({
                    "file_path": file_path.to_str().unwrap()
                }),
            )
            .await;

        assert!(result.is_ok());
        let output = result.unwrap().as_text().unwrap().to_string();
        assert!(output.contains("TOO LARGE"));
        assert!(output.contains("start_line"));
    }

    #[tokio::test]
    async fn test_read_file_tool_string_coercion() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "Line 1\nLine 2\nLine 3").unwrap();

        let tool = ReadFileTool;
        let ctx = RunContext::minimal("test");

        // Test with string numbers (LLM sometimes does this)
        let result = tool
            .call(
                &ctx,
                serde_json::json!({
                    "file_path": file_path.to_str().unwrap(),
                    "start_line": "1",
                    "num_lines": "2"
                }),
            )
            .await;

        assert!(result.is_ok());
        let output = result.unwrap().as_text().unwrap().to_string();
        assert!(output.contains("Line 1"));
        assert!(output.contains("Line 2"));
    }
}
