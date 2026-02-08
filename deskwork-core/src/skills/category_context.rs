//! Unified context builder for skill categories.
//!
//! Builds the system prompt section that injects knowledge from enabled
//! skill categories, connector status, and MCP info.

use std::collections::HashSet;

use crate::skills::categories::{McpBridgeResult, SkillCategoryRegistry};

/// Built prompt context for injection into the system prompt.
#[derive(Debug, Clone)]
pub struct CategoryContext {
    pub prompt: String,
    pub estimated_tokens: usize,
    pub truncated: bool,
}

/// Budget controlling how many tokens the category context may use.
#[derive(Debug, Clone, Copy)]
pub struct ContextBudget {
    pub max_tokens: usize,
}

impl Default for ContextBudget {
    fn default() -> Self {
        Self { max_tokens: 6_000 }
    }
}

/// Build the system prompt injection block for enabled skill categories.
///
/// This is the category-based successor to the old plugin context builder.
pub fn build_category_context(
    registry: &SkillCategoryRegistry,
    mcp: &McpBridgeResult,
    budget: ContextBudget,
) -> CategoryContext {
    let mut enabled_categories = registry.enabled_categories();
    enabled_categories.sort_by(|a, b| a.name.cmp(&b.name));

    let mut out = String::new();
    out.push_str("## Skill Categories\n\n");

    if enabled_categories.is_empty() {
        out.push_str("No skill categories enabled.\n");
        return CategoryContext {
            estimated_tokens: estimate_tokens(&out),
            prompt: out,
            truncated: false,
        };
    }

    let available_connectors = available_connector_names(mcp);

    let mut base_sections = String::new();
    let mut skill_sections: Vec<String> = Vec::new();

    base_sections.push_str("Enabled categories:\n");

    for category in &enabled_categories {
        base_sections.push_str(&format!(
            "- {} (`{}`)\n  - {}\n",
            category.name, category.id, category.description
        ));

        // Slash commands
        base_sections.push_str("  - Commands:\n");
        if category.commands.is_empty() {
            base_sections.push_str("    - none\n");
        } else {
            let mut commands = category.commands.iter().collect::<Vec<_>>();
            commands.sort_by(|a, b| a.slash_command.cmp(&b.slash_command));

            for command in commands {
                match command.argument_hint.as_deref() {
                    Some(hint) if !hint.trim().is_empty() => base_sections.push_str(&format!(
                        "    - {}: {} (args: {})\n",
                        command.slash_command,
                        command.description,
                        hint.trim()
                    )),
                    _ => base_sections.push_str(&format!(
                        "    - {}: {}\n",
                        command.slash_command, command.description
                    )),
                }
            }
        }

        // Connector docs (always included, even under truncation).
        base_sections.push_str("  - Connector notes:\n");
        if category.connectors_doc.trim().is_empty() {
            base_sections.push_str("    [none]\n");
        } else {
            let (resolved, notes) =
                resolve_category_placeholders(category.connectors_doc.trim(), &available_connectors);

            base_sections.push_str("```markdown\n");
            base_sections.push_str(resolved.trim());
            base_sections.push_str("\n```\n");
            append_placeholder_notes(&mut base_sections, &notes, 4);
        }

        // Include category issues if they exist (even if category is active).
        if !category.errors.is_empty() {
            base_sections.push_str("  - Category issues:\n");
            for error in &category.errors {
                base_sections.push_str(&format!("    - {}\n", error));
            }
        }

        // Skill sections (these are lower-priority and get truncated first).
        for skill in &category.skills {
            let mut section = String::new();

            let (resolved_skill, notes) =
                resolve_category_placeholders(&skill.content, &available_connectors);

            section.push_str(&format!(
                "\n### Skill: {} ({})\n",
                skill.name,
                skill.path.display()
            ));
            section.push_str(&format!(
                "Category: {} (`{}`)\n",
                category.name, category.id
            ));
            section.push_str(&format!("Description: {}\n", skill.description));
            section.push_str("```markdown\n");
            section.push_str(resolved_skill.trim());
            section.push_str("\n```\n");
            append_placeholder_notes(&mut section, &notes, 0);

            skill_sections.push(section);
        }
    }

    // MCP connector status
    let mut mcp_section = String::new();
    mcp_section.push_str("\n## MCP Connectors\n\n");
    if mcp.configs.is_empty() {
        mcp_section.push_str("No active MCP connectors configured.\n");
    } else {
        let mut configs = mcp.configs.iter().collect::<Vec<_>>();
        configs.sort_by(|(a, _), (b, _)| a.cmp(b));

        mcp_section.push_str("Available connectors:\n");
        for (name, cfg) in configs {
            mcp_section.push_str(&format!("- `{name}` ({})\n", cfg.r#type));
        }
    }

    // Token budget management: drop lower-priority skills first (from the end).
    let mut truncated = false;
    let mut included_skills = skill_sections;

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

    // If still too large (base/connector docs alone), keep it and mark truncated.
    if estimate_tokens(&out) > budget.max_tokens {
        truncated = true;
    }

    if truncated {
        out.push_str("\n[category context truncated: lower-priority skills omitted to fit budget]\n");
    }

    CategoryContext {
        estimated_tokens: estimate_tokens(&out),
        prompt: out,
        truncated,
    }
}

/// Extract available connector names (lowercased, without category namespace) from the MCP bridge.
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

/// Resolve `~~category` placeholders in markdown, based on which connectors are configured.
///
/// Returns `(resolved_text, notes)`.
fn resolve_category_placeholders(
    input: &str,
    connectors: &HashSet<String>,
) -> (String, Vec<String>) {
    let mut out = input.to_string();
    let mut notes = Vec::new();

    // Keep this table centralized and boring. Boring code is good code.
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
        // New categories for finance/data plugins:
        (
            "data warehouse",
            vec!["~~data warehouse", "~~Data warehouse", "~~Data Warehouse"],
            vec!["snowflake", "databricks", "bigquery", "redshift", "postgresql"],
        ),
        (
            "erp",
            vec!["~~erp", "~~ERP"],
            vec!["netsuite", "sap", "quickbooks", "xero"],
        ),
        (
            "analytics",
            vec!["~~analytics", "~~Analytics"],
            vec!["tableau", "looker", "powerbi", "power-bi"],
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
    text.chars().count().div_ceil(4)
}
