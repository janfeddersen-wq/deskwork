//! Slash command parsing, suggestions, and prompt building.
//!
//! Commands use the format `/{category_id}:{command_name}` (e.g., `/legal:review-contract`).

use std::collections::HashMap;

use crate::skills::types::CommandFile; // reuse existing type for now
use crate::skills::categories::SkillCategoryRegistry;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedSlashCommand {
    pub category_id: String,
    pub command_name: String,
    pub slash_command: String,
    pub raw_args: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlashCommandSuggestion {
    pub slash_command: String,
    pub category_id: String,
    pub command_name: String,
    pub description: String,
}

/// Parse a slash command from user input.
///
/// Expected format: `/{category_id}:{command_name} [optional args]`
///
/// Returns `None` if input doesn't match the format.
pub fn parse_slash_command(input: &str) -> Option<ParsedSlashCommand> {
    let trimmed = input.trim();
    if !trimmed.starts_with('/') {
        return None;
    }

    let (cmd_part, arg_part) = match trimmed.split_once(' ') {
        Some((cmd, rest)) => (cmd, Some(rest.trim().to_string())),
        None => (trimmed, None),
    };

    let body = &cmd_part[1..];
    let (category_id, command_name) = body.split_once(':')?;

    if category_id.is_empty() || command_name.is_empty() {
        return None;
    }

    Some(ParsedSlashCommand {
        category_id: category_id.to_string(),
        command_name: command_name.to_string(),
        slash_command: format!("/{category_id}:{command_name}"),
        raw_args: arg_part.filter(|s| !s.is_empty()),
    })
}

/// Get command suggestions matching a prefix, using the category registry.
pub fn command_suggestions(registry: &SkillCategoryRegistry, prefix: &str) -> Vec<String> {
    command_suggestions_rich(registry, prefix)
        .into_iter()
        .map(|s| s.slash_command)
        .collect()
}

/// Get rich command suggestions with descriptions, using the category registry.
pub fn command_suggestions_rich(
    registry: &SkillCategoryRegistry,
    prefix: &str,
) -> Vec<SlashCommandSuggestion> {
    let normalized = prefix.trim().to_ascii_lowercase();

    let mut ranked = registry
        .all_slash_commands()
        .into_iter()
        .filter_map(|command| {
            let slash = command.slash_command.to_ascii_lowercase();
            let desc = command.description.to_ascii_lowercase();

            let score = if slash.starts_with(&normalized) {
                0
            } else if slash.contains(&normalized) {
                1
            } else if desc.contains(&normalized) {
                2
            } else {
                return None;
            };

            Some((
                score,
                SlashCommandSuggestion {
                    slash_command: command.slash_command.clone(),
                    category_id: command.plugin_id.clone(), // plugin_id holds category_id
                    command_name: command.name.clone(),
                    description: command.description.clone(),
                },
            ))
        })
        .collect::<Vec<_>>();

    ranked.sort_by(|(score_a, a), (score_b, b)| {
        score_a
            .cmp(score_b)
            .then_with(|| a.category_id.cmp(&b.category_id))
            .then_with(|| a.command_name.cmp(&b.command_name))
    });

    ranked
        .into_iter()
        .map(|(_, suggestion)| suggestion)
        .collect()
}

/// Build the full prompt that gets sent to the model when a slash command is invoked.
pub fn build_command_prompt(
    command: &CommandFile,
    user_inputs: &HashMap<String, String>,
    raw_args: Option<&str>,
) -> String {
    let mut body = command.content.clone();

    for (key, value) in user_inputs {
        let needle = format!("{{{{{key}}}}}");
        body = body.replace(&needle, value);
    }

    let mut prompt = String::new();
    prompt.push_str(&format!("# Slash Command\n{}\n\n", command.slash_command));
    prompt.push_str(&format!("Description: {}\n", command.description));

    if let Some(hint) = command.argument_hint.as_deref() {
        prompt.push_str(&format!("Argument hint: {}\n", hint));
    }

    if let Some(args) = raw_args.filter(|v| !v.trim().is_empty()) {
        prompt.push_str(&format!("Raw user args: {}\n", args.trim()));
    }

    if !user_inputs.is_empty() {
        prompt.push_str("\nParsed user inputs:\n");
        for (key, value) in user_inputs {
            prompt.push_str(&format!("- {}: {}\n", key, value));
        }
    }

    prompt.push_str("\n## Command Template\n");
    prompt.push_str(body.trim());
    prompt.push('\n');

    prompt
}

/// Get the command handler for a specific slash command string from the category registry.
pub fn get_command_handler<'a>(
    registry: &'a SkillCategoryRegistry,
    slash_command: &str,
) -> Option<&'a CommandFile> {
    registry
        .all_slash_commands()
        .into_iter()
        .find(|command| command.slash_command == slash_command)
}
