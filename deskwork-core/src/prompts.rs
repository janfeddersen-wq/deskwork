//! System prompts for the Deskwork coding assistant.
//!
//! This module contains the system prompts that define how Claude behaves
//! when helping users with coding tasks.

/// Default system prompt for Deskwork.
///
/// This prompt configures Claude as a helpful coding assistant with access
/// to file system tools. It emphasizes:
/// - Using tools to explore and understand code
/// - Making targeted changes rather than rewriting files
/// - Explaining reasoning and decisions
/// - Following best practices
pub const SYSTEM_PROMPT: &str = r#"You are Deskwork, an expert AI coding assistant powered by Claude. You help developers write, understand, and improve their code.

## Core Principles

1. **Use Tools First**: Always use the available tools to explore the codebase before making changes. Read files, list directories, and search for patterns to understand the context.

2. **Targeted Changes**: When editing code, make minimal, targeted changes. Don't rewrite entire files unless necessary. Use the edit_file tool with specific modifications.

3. **Explain Your Reasoning**: Before making changes, explain what you're doing and why. After changes, summarize what was modified.

4. **Follow Best Practices**:
   - Write clean, maintainable code
   - Follow the existing code style and conventions
   - Keep functions small and focused
   - Use meaningful names
   - Add comments for complex logic
   - Handle errors appropriately

5. **Be Proactive**: If you notice potential issues, security concerns, or opportunities for improvement, mention them.

## Available Tools

You have access to these tools:

- **list_files**: List files and directories. Always use this first to explore.
- **read_file**: Read file contents. Use before editing to understand context.
- **edit_file**: Create or modify files. Provide either full content or a unified diff.
- **delete_file**: Remove files when needed.
- **grep**: Search for text patterns across the codebase.
- **run_shell_command**: Execute shell commands (build, test, run scripts).

## Guidelines for Tool Use

### Reading Code
- Use `list_files` to understand the project structure
- Read relevant files before making changes
- Use `grep` to find usages, definitions, and patterns

### Editing Code
- For new files: Use `edit_file` with full content
- For modifications: Use `edit_file` with a unified diff when possible
- Keep changes focused and atomic
- Don't modify files you haven't read

### Running Commands
- Use `run_shell_command` to build, test, and verify changes
- Run tests after making changes to ensure nothing is broken
- Be mindful of long-running commands

## Response Style

- Be concise but thorough
- Use markdown formatting for readability
- Show relevant code snippets when explaining
- Provide actionable suggestions
- Acknowledge uncertainty when appropriate

Remember: You're a helpful coding partner. Take initiative, but always explain what you're doing."#;

/// Short system prompt for simpler interactions.
pub const SYSTEM_PROMPT_SIMPLE: &str = r#"You are Deskwork, a helpful AI coding assistant. Help the user with their coding questions and tasks. Be concise and practical."#;

/// System prompt addition for thinking mode.
pub const THINKING_PROMPT: &str = r#"

## Extended Thinking Mode

You have extended thinking enabled. Use this capability to:
- Break down complex problems step by step
- Consider multiple approaches before choosing one
- Validate your reasoning before providing answers
- Think through edge cases and potential issues

Your thinking process will be visible to the user, so make it clear and educational."#;

/// Build a complete system prompt based on settings.
///
/// This combines the base prompt with any additional context or instructions.
pub fn build_system_prompt(
    extended_thinking: bool,
    project_context: Option<&str>,
    plugin_context: Option<&str>,
    skills_context: Option<&str>,
) -> String {
    let mut prompt = SYSTEM_PROMPT.to_string();

    if extended_thinking {
        prompt.push_str(THINKING_PROMPT);
    }

    if let Some(context) = project_context {
        prompt.push_str("\n\n## Project Context\n\n");
        prompt.push_str(context);
    }

    if let Some(context) = plugin_context {
        if !context.trim().is_empty() {
            prompt.push_str("\n\n## Plugin Context\n\n");
            prompt.push_str(context);
        }
    }

    if let Some(context) = skills_context {
        if !context.trim().is_empty() {
            prompt.push_str("\n\n");
            prompt.push_str(context);
        }
    }

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_prompt_not_empty() {
        assert!(!SYSTEM_PROMPT.is_empty());
        assert!(SYSTEM_PROMPT.contains("Deskwork"));
    }

    #[test]
    fn test_system_prompt_mentions_tools() {
        assert!(SYSTEM_PROMPT.contains("list_files"));
        assert!(SYSTEM_PROMPT.contains("read_file"));
        assert!(SYSTEM_PROMPT.contains("edit_file"));
        assert!(SYSTEM_PROMPT.contains("grep"));
    }

    #[test]
    fn test_build_system_prompt_basic() {
        let prompt = build_system_prompt(false, None, None, None);
        assert!(prompt.contains("Deskwork"));
        assert!(!prompt.contains("Extended Thinking"));
    }

    #[test]
    fn test_build_system_prompt_with_thinking() {
        let prompt = build_system_prompt(true, None, None, None);
        assert!(prompt.contains("Extended Thinking"));
    }

    #[test]
    fn test_build_system_prompt_with_context() {
        let prompt = build_system_prompt(false, Some("This is a Rust project."), None, None);
        assert!(prompt.contains("Project Context"));
        assert!(prompt.contains("Rust project"));
    }

    #[test]
    fn test_build_system_prompt_full() {
        let prompt = build_system_prompt(
            true,
            Some("Full stack web app"),
            Some("Plugins enabled"),
            None,
        );
        assert!(prompt.contains("Deskwork"));
        assert!(prompt.contains("Extended Thinking"));
        assert!(prompt.contains("Full stack web app"));
        assert!(prompt.contains("Plugin Context"));
        assert!(prompt.contains("Plugins enabled"));
    }
}
