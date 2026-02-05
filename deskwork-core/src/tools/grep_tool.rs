//! Grep tool implementation.
//!
//! Provides a serdesAI-compatible tool for searching text patterns across files.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use tracing::debug;

use serdes_ai_tools::{RunContext, SchemaBuilder, Tool, ToolDefinition, ToolResult, ToolReturn};

use super::file_ops;

/// Tool for searching text patterns across files.
#[derive(Debug, Clone, Default)]
pub struct GrepTool;

#[derive(Debug, Deserialize)]
struct GrepArgs {
    pattern: String,
    directory: Option<String>,
    max_results: Option<usize>,
}

#[async_trait]
impl Tool for GrepTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "grep",
            "Recursively search for text patterns across files. \
             Searches recognized text file types while limiting results for performance. \
             Safety: max 100 matches, lines truncated at 512 chars, files over 5MB skipped.",
        )
        .with_parameters(
            SchemaBuilder::new()
                .string(
                    "pattern",
                    "The text pattern to search for. Supports regex. Use --ignore-case or -i prefix for case-insensitive.",
                    true,
                )
                .string(
                    "directory",
                    "Root directory to start the recursive search. Defaults to '.'.",
                    false,
                )
                .integer(
                    "max_results",
                    "Maximum number of matches to return. Defaults to 100.",
                    false,
                )
                .build()
                .expect("schema build failed"),
        )
    }

    async fn call(&self, _ctx: &RunContext, args: JsonValue) -> ToolResult {
        debug!(tool = "grep", ?args, "Tool called");

        let args: GrepArgs = super::common::parse_tool_args_lenient(
            "grep",
            args.clone(),
            self.definition().parameters(),
        )?;

        let directory = args.directory.as_deref().unwrap_or(".");

        match file_ops::grep(&args.pattern, directory, args.max_results) {
            Ok(result) => {
                if result.matches.is_empty() {
                    return Ok(ToolReturn::text(format!(
                        "No matches found for pattern '{}' in {}",
                        args.pattern, directory
                    )));
                }

                let mut output = format!(
                    "Found {} matches for '{}' in {}:\n",
                    result.total_matches, args.pattern, directory
                );

                for m in &result.matches {
                    output.push_str(&format!("\n{}:{}:{}", m.path, m.line_number, m.content));
                }

                Ok(ToolReturn::text(output))
            }
            Err(e) => Ok(ToolReturn::error(format!("Grep failed: {}", e))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_grep_finds_matches() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "hello world\nfoo bar\nhello again").unwrap();

        let tool = GrepTool;
        let ctx = RunContext::minimal("test");

        let result = tool
            .call(
                &ctx,
                serde_json::json!({
                    "pattern": "hello",
                    "directory": dir.path().to_str().unwrap()
                }),
            )
            .await;

        assert!(result.is_ok());
        let ret = result.unwrap();
        assert!(!ret.is_error());
        let text = ret.as_text().unwrap();
        assert!(text.contains("Found"));
        assert!(text.contains("hello"));
    }

    #[tokio::test]
    async fn test_grep_no_matches() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "hello world").unwrap();

        let tool = GrepTool;
        let ctx = RunContext::minimal("test");

        let result = tool
            .call(
                &ctx,
                serde_json::json!({
                    "pattern": "notfound",
                    "directory": dir.path().to_str().unwrap()
                }),
            )
            .await;

        assert!(result.is_ok());
        let ret = result.unwrap();
        let text = ret.as_text().unwrap();
        assert!(text.contains("No matches found"));
    }

    #[tokio::test]
    async fn test_grep_invalid_directory() {
        let tool = GrepTool;
        let ctx = RunContext::minimal("test");

        let result = tool
            .call(
                &ctx,
                serde_json::json!({
                    "pattern": "test",
                    "directory": "/nonexistent/path/xyz123"
                }),
            )
            .await;

        assert!(result.is_ok());
        let ret = result.unwrap();
        assert!(ret.is_error());
    }

    #[tokio::test]
    async fn test_grep_case_insensitive() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test.txt");
        fs::write(&file_path, "Hello World\nHELLO\nhello").unwrap();

        let tool = GrepTool;
        let ctx = RunContext::minimal("test");

        let result = tool
            .call(
                &ctx,
                serde_json::json!({
                    "pattern": "-i hello",
                    "directory": dir.path().to_str().unwrap()
                }),
            )
            .await;

        assert!(result.is_ok());
        let ret = result.unwrap();
        let text = ret.as_text().unwrap();
        assert!(text.contains("Found 3 matches"));
    }
}
