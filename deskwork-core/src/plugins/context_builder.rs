use std::collections::HashSet;

use crate::plugins::mcp_bridge::McpBridgeResult;
use crate::plugins::registry::PluginRegistry;

#[derive(Debug, Clone)]
pub struct PluginContext {
    pub prompt: String,
    pub estimated_tokens: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ContextBudget {
    pub max_tokens: usize,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self { max_tokens: 2_000 }
    }
}

pub fn build_plugin_prompt_block(
    registry: &PluginRegistry,
    mcp: &McpBridgeResult,
    budget: ContextBudget,
) -> PluginContext {
    let enabled_plugins = registry.enabled_plugins();

    let mut out = String::new();
    out.push_str("## Plugins\n\n");

    if enabled_plugins.is_empty() {
        out.push_str("No plugins are currently enabled.\n");
        return PluginContext {
            estimated_tokens: estimate_tokens(&out),
            prompt: out,
            truncated: false,
        };
    }

    let available_connectors = available_connector_names(mcp);
    let mut base_sections = String::new();
    let mut skill_sections: Vec<String> = Vec::new();

    base_sections.push_str("Enabled plugins:\n");
    for plugin in &enabled_plugins {
        base_sections.push_str(&format!(
            "- {} (`{}`) v{}\n  - {}\n",
            plugin.name, plugin.id, plugin.version, plugin.description
        ));

        base_sections.push_str("  - Commands:\n");
        if plugin.commands.is_empty() {
            base_sections.push_str("    - none\n");
        } else {
            for command in &plugin.commands {
                base_sections.push_str(&format!(
                    "    - {}: {}\n",
                    command.slash_command, command.description
                ));
            }
        }

        // Always include local config, even under truncation.
        base_sections.push_str("  - Local config:\n");
        match plugin.local_config.as_deref() {
            Some(local) if !local.trim().is_empty() => {
                let (resolved_local, notes) =
                    resolve_category_placeholders(local.trim(), &available_connectors);
                base_sections.push_str("```markdown\n");
                base_sections.push_str(&resolved_local);
                base_sections.push_str("\n```\n");
                append_placeholder_notes(&mut base_sections, &notes, 4);
            }
            _ => base_sections.push_str("    [none]\n"),
        }

        if !plugin.errors.is_empty() {
            base_sections.push_str("  - Plugin issues:\n");
            for error in &plugin.errors {
                base_sections.push_str(&format!("    - {}\n", error));
            }
        }

        if !plugin.skills.is_empty() {
            for skill in &plugin.skills {
                let mut section = String::new();
                let (resolved_skill, notes) =
                    resolve_category_placeholders(&skill.content, &available_connectors);

                section.push_str(&format!(
                    "\n### Skill: {} ({})\n",
                    skill.name,
                    skill.path.display()
                ));
                section.push_str(&format!("Description: {}\n", skill.description));
                section.push_str("```markdown\n");
                section.push_str(resolved_skill.trim());
                section.push_str("\n```\n");
                append_placeholder_notes(&mut section, &notes, 0);

                skill_sections.push(section);
            }
        }
    }

    let mut mcp_section = String::new();
    mcp_section.push_str("\n## MCP Connectors\n\n");
    if mcp.configs.is_empty() && mcp.unavailable.is_empty() {
        mcp_section.push_str("No active MCP connectors configured.\n");
    } else {
        if !mcp.configs.is_empty() {
            mcp_section.push_str("Available connectors:\n");
            for (name, cfg) in &mcp.configs {
                mcp_section.push_str(&format!("- `{name}` ({})\n", cfg.r#type));
            }
        }

        if !mcp.unavailable.is_empty() {
            mcp_section.push_str("Unavailable connectors:\n");
            for item in &mcp.unavailable {
                mcp_section.push_str(&format!("- `{}`: {}\n", item.namespaced_name, item.reason));
            }
        }
    }

    let mut truncated = false;
    let mut included_skills = skill_sections;

    // Trim lower-priority skills first (from the end).
    loop {
        let mut candidate = String::new();
        candidate.push_str(&out);
        candidate.push_str(&base_sections);
        for skill in &included_skills {
            candidate.push_str(skill);
        }
        candidate.push_str(&mcp_section);

        if estimate_tokens(&candidate) <= budget.max_tokens || included_skills.is_empty() {
            out = candidate;
            break;
        }

        included_skills.pop();
        truncated = true;
    }

    // If still too large (base/local config alone), keep it and mark truncated.
    let estimated = estimate_tokens(&out);
    if estimated > budget.max_tokens {
        truncated = true;
    }

    if truncated {
        out.push_str("\n[plugin context truncated: lower-priority skills omitted to fit budget]\n");
    }

    PluginContext {
        estimated_tokens: estimate_tokens(&out),
        prompt: out,
        truncated,
    }
}

fn available_connector_names(mcp: &McpBridgeResult) -> HashSet<String> {
    mcp.configs
        .keys()
        .map(|name| {
            name.split_once(':')
                .map(|(_, connector)| connector)
                .unwrap_or(name)
                .to_ascii_lowercase()
        })
        .collect()
}

fn resolve_category_placeholders(
    input: &str,
    connectors: &HashSet<String>,
) -> (String, Vec<String>) {
    let mut out = input.to_string();
    let mut notes = Vec::new();

    let categories = [
        (
            "cloud storage",
            vec!["~~cloud storage", "~~Cloud storage", "~~Cloud Storage"],
            vec![
                "box",
                "egnyte",
                "sharepoint",
                "onedrive",
                "dropbox",
                "gdrive",
                "google-drive",
            ],
        ),
        (
            "chat",
            vec!["~~chat", "~~Chat"],
            vec!["slack", "teams", "ms-teams"],
        ),
        (
            "office suite",
            vec!["~~office suite", "~~Office suite", "~~Office Suite"],
            vec!["ms365", "microsoft365", "google-workspace", "workspace"],
        ),
        (
            "project tracking",
            vec![
                "~~project tracker",
                "~~Project tracker",
                "~~project management",
                "~~Project management",
            ],
            vec!["atlassian", "jira", "confluence", "linear", "asana"],
        ),
        (
            "clm",
            vec!["~~clm", "~~CLM"],
            vec!["clm", "ironclad", "agiloft"],
        ),
        (
            "crm",
            vec!["~~crm", "~~CRM"],
            vec!["crm", "salesforce", "hubspot"],
        ),
        (
            "e-signature",
            vec!["~~e-signature", "~~E-signature", "~~E-Signature"],
            vec!["esignature", "e-signature", "docusign", "adobe-sign"],
        ),
        (
            "email",
            vec!["~~email", "~~Email"],
            vec!["outlook", "gmail", "email", "ms365"],
        ),
        (
            "calendar",
            vec!["~~calendar", "~~Calendar"],
            vec!["calendar", "outlook-calendar", "gcal", "ms365"],
        ),
    ];

    for (category, placeholders, candidates) in categories {
        let configured = candidates
            .iter()
            .filter(|name| connectors.contains(**name))
            .map(|name| (*name).to_string())
            .collect::<Vec<_>>();

        for placeholder in placeholders {
            if !out.contains(placeholder) {
                continue;
            }

            if configured.is_empty() {
                out = out.replace(placeholder, "[not configured]");
                notes.push(format!(
                    "No connector configured for category `{category}` (placeholder `{placeholder}`)."
                ));
            } else {
                out = out.replace(placeholder, &configured.join("/"));
            }
        }
    }

    (out, notes)
}

fn append_placeholder_notes(out: &mut String, notes: &[String], indent: usize) {
    if notes.is_empty() {
        return;
    }

    let pad = " ".repeat(indent);
    out.push_str(&format!("{pad}Placeholder notes:\n"));
    for note in notes {
        out.push_str(&format!("{pad}- {}\n", note));
    }
}

fn estimate_tokens(text: &str) -> usize {
    // Very rough heuristic: 1 token ~= 4 chars for English-ish text.
    text.chars().count().div_ceil(4)
}
