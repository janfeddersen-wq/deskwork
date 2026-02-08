//! RunShellCommand tool implementation.
//!
//! Provides a serdesAI-compatible tool for executing shell commands.

use std::process::Stdio;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value as JsonValue;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::time::{timeout, Duration};
use tracing::{debug, info};

use serdes_ai_tools::{RunContext, SchemaBuilder, Tool, ToolDefinition, ToolResult, ToolReturn};

/// Tool for executing shell commands.
#[derive(Debug, Clone, Default)]
pub struct RunShellCommandTool;

/// Default timeout for shell commands (60 seconds).
const DEFAULT_TIMEOUT_SECS: u64 = 60;

/// Maximum characters in shell output.
const MAX_OUTPUT_CHARS: usize = 50_000;

#[derive(Debug, Deserialize)]
struct RunShellCommandArgs {
    command: String,
    working_directory: Option<String>,
    timeout_seconds: Option<u64>,
}

#[async_trait]
impl Tool for RunShellCommandTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition::new(
            "run_shell_command",
            "Execute a shell command and return the output. \
             Commands run asynchronously with timeout protection.",
        )
        .with_parameters(
            SchemaBuilder::new()
                .string("command", "The shell command to execute.", true)
                .string(
                    "working_directory",
                    "Working directory for command execution. Defaults to current directory.",
                    false,
                )
                .integer(
                    "timeout_seconds",
                    "Maximum time to wait for command completion. Defaults to 60 seconds.",
                    false,
                )
                .build()
                .expect("schema build failed"),
        )
    }

    async fn call(&self, _ctx: &RunContext, args: JsonValue) -> ToolResult {
        debug!(tool = "run_shell_command", ?args, "Tool called");

        let args: RunShellCommandArgs = super::common::parse_tool_args_lenient(
            "run_shell_command",
            args.clone(),
            self.definition().parameters(),
        )?;

        let timeout_secs = args.timeout_seconds.unwrap_or(DEFAULT_TIMEOUT_SECS);

        info!(
            tool = "run_shell_command",
            command = %args.command,
            working_directory = ?args.working_directory,
            timeout = timeout_secs,
            "Executing command"
        );

        match execute_command(
            &args.command,
            args.working_directory.as_deref(),
            timeout_secs,
        )
        .await
        {
            Ok(result) => Ok(ToolReturn::text(result)),
            Err(e) => Ok(ToolReturn::error(format!(
                "Command execution failed: {}",
                e
            ))),
        }
    }
}

async fn execute_command(
    command: &str,
    working_directory: Option<&str>,
    timeout_secs: u64,
) -> Result<String, String> {
    let shell = if cfg!(windows) { "cmd" } else { "sh" };
    let shell_arg = if cfg!(windows) { "/C" } else { "-c" };

    let mut cmd = Command::new(shell);
    cmd.arg(shell_arg).arg(command);

    if let Some(dir) = working_directory {
        let path = std::path::Path::new(dir);
        if !path.exists() {
            return Err(format!("Working directory does not exist: {}", dir));
        }
        cmd.current_dir(dir);
    }

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    // Set headless environment
    cmd.env("TERM", "dumb");
    cmd.env("NO_COLOR", "1");
    cmd.env("CLICOLOR", "0");

    let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn: {}", e))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    // Collect output with timeout
    let output_future = async {
        let mut stdout_lines = Vec::new();
        let mut stderr_lines = Vec::new();

        if let Some(stdout) = stdout {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                stdout_lines.push(line);
            }
        }

        if let Some(stderr) = stderr {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                stderr_lines.push(line);
            }
        }

        let exit_status = child.wait().await;
        (stdout_lines, stderr_lines, exit_status)
    };

    let result = timeout(Duration::from_secs(timeout_secs), output_future).await;

    match result {
        Ok((stdout_lines, stderr_lines, exit_status)) => {
            let exit_code = exit_status.map(|s| s.code().unwrap_or(-1)).unwrap_or(-1);

            let success = exit_code == 0;

            let mut output = String::new();

            if success {
                output.push_str(&format!(
                    "Command completed successfully (exit code: {})\n",
                    exit_code
                ));
            } else {
                output.push_str(&format!("Command failed (exit code: {})\n", exit_code));
            }

            if !stdout_lines.is_empty() {
                output.push_str("\n--- stdout ---\n");
                output.push_str(&stdout_lines.join("\n"));
            }

            if !stderr_lines.is_empty() {
                output.push_str("\n--- stderr ---\n");
                output.push_str(&stderr_lines.join("\n"));
            }

            // Truncate if needed
            if output.len() > MAX_OUTPUT_CHARS {
                output.truncate(MAX_OUTPUT_CHARS);
                output.push_str("\n\n[OUTPUT TRUNCATED]");
            }

            Ok(output)
        }
        Err(_) => {
            // Timeout - try to kill process
            let _ = child.kill().await;
            Err(format!("Command timed out after {} seconds", timeout_secs))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_shell_echo() {
        let tool = RunShellCommandTool;
        let ctx = RunContext::minimal("test");

        let result = tool
            .call(&ctx, serde_json::json!({ "command": "echo hello" }))
            .await;

        assert!(result.is_ok());
        let ret = result.unwrap();
        assert!(!ret.is_error());
        let text = ret.as_text().unwrap();
        assert!(text.contains("successfully"));
        assert!(text.contains("hello"));
    }

    #[tokio::test]
    async fn test_shell_exit_code() {
        let tool = RunShellCommandTool;
        let ctx = RunContext::minimal("test");

        let result = tool
            .call(&ctx, serde_json::json!({ "command": "exit 42" }))
            .await;

        assert!(result.is_ok());
        let ret = result.unwrap();
        let text = ret.as_text().unwrap();
        assert!(text.contains("failed"));
        assert!(text.contains("42"));
    }

    #[tokio::test]
    async fn test_shell_invalid_working_directory() {
        let tool = RunShellCommandTool;
        let ctx = RunContext::minimal("test");

        let result = tool
            .call(
                &ctx,
                serde_json::json!({
                    "command": "echo test",
                    "working_directory": "/nonexistent/path"
                }),
            )
            .await;

        assert!(result.is_ok());
        let ret = result.unwrap();
        assert!(ret.is_error());
    }

    #[tokio::test]
    async fn test_shell_captures_stderr() {
        let tool = RunShellCommandTool;
        let ctx = RunContext::minimal("test");

        let result = tool
            .call(&ctx, serde_json::json!({ "command": "echo error >&2" }))
            .await;

        assert!(result.is_ok());
        let ret = result.unwrap();
        let text = ret.as_text().unwrap();
        assert!(text.contains("stderr"));
        assert!(text.contains("error"));
    }
}
