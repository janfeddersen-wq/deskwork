use std::collections::HashMap;
use std::path::PathBuf;

use crate::plugins::types::{
    CommandFile, McpServersFile, Plugin, PluginAuthor, PluginManifest, PluginStatus, SkillFile,
};

mod generated {
    include!(concat!(env!("OUT_DIR"), "/bundled_legal_assets.rs"));
}

use generated::*;

const DEFAULT_LEGAL_LOCAL_CONFIG: &str = r#"# legal.local.md

This file contains organization-specific legal playbook settings.

## Contract Review Playbook

- Standard positions: Define your preferred clause language for key contract terms.
- Acceptable ranges: Document terms that are acceptable without escalation.
- Escalation triggers: Define terms requiring senior counsel / outside counsel review.

## Connector setup

Set these environment variables to enable optional legal connectors:

- `LEGAL_CLM_MCP_URL`
- `LEGAL_CRM_MCP_URL`
- `LEGAL_ESIGN_MCP_URL`

## Notes

- Keep this file in source control only if appropriate for your organization.
- Include sensitive or privileged details only in secure environments.
"#;

pub fn load_bundled_legal_plugin() -> Plugin {
    let mut errors = Vec::new();

    let manifest = if LEGAL_PLUGIN_MANIFEST.trim().is_empty() {
        errors.push(
            "Bundled legal plugin manifest is unavailable (source files missing at build time)"
                .to_string(),
        );
        PluginManifest {
            name: "legal".to_string(),
            version: "0.0.0".to_string(),
            description: "Bundled legal plugin unavailable".to_string(),
            author: PluginAuthor {
                name: "Unknown".to_string(),
            },
        }
    } else {
        match serde_json::from_str::<PluginManifest>(LEGAL_PLUGIN_MANIFEST) {
            Ok(manifest) => manifest,
            Err(err) => {
                errors.push(format!(
                    "Failed parsing bundled legal plugin manifest: {err}"
                ));
                PluginManifest {
                    name: "legal".to_string(),
                    version: "0.0.0".to_string(),
                    description: "Bundled legal plugin (manifest parse fallback)".to_string(),
                    author: PluginAuthor {
                        name: "Unknown".to_string(),
                    },
                }
            }
        }
    };

    let mcp_servers = if LEGAL_PLUGIN_MCP.trim().is_empty() {
        HashMap::new()
    } else {
        match serde_json::from_str::<McpServersFile>(LEGAL_PLUGIN_MCP) {
            Ok(file) => file.mcp_servers,
            Err(err) => {
                errors.push(format!("Failed parsing bundled legal MCP config: {err}"));
                HashMap::new()
            }
        }
    };

    let plugin_id = "legal".to_string();

    let skill_assets = vec![
        (
            "bundled/legal/skills/canned-responses/SKILL.md",
            LEGAL_SKILL_CANNED_RESPONSES,
        ),
        (
            "bundled/legal/skills/compliance/SKILL.md",
            LEGAL_SKILL_COMPLIANCE,
        ),
        (
            "bundled/legal/skills/contract-review/SKILL.md",
            LEGAL_SKILL_CONTRACT_REVIEW,
        ),
        (
            "bundled/legal/skills/legal-risk-assessment/SKILL.md",
            LEGAL_SKILL_RISK_ASSESSMENT,
        ),
        (
            "bundled/legal/skills/meeting-briefing/SKILL.md",
            LEGAL_SKILL_MEETING_BRIEFING,
        ),
        (
            "bundled/legal/skills/nda-triage/SKILL.md",
            LEGAL_SKILL_NDA_TRIAGE,
        ),
    ];

    let skills = skill_assets
        .into_iter()
        .filter_map(|(path, content)| {
            if content.trim().is_empty() {
                return None;
            }
            Some(SkillFile::from_markdown(path, content))
        })
        .collect::<Vec<_>>();

    let command_assets = vec![
        ("bundled/legal/commands/brief.md", LEGAL_COMMAND_BRIEF),
        ("bundled/legal/commands/respond.md", LEGAL_COMMAND_RESPOND),
        (
            "bundled/legal/commands/review-contract.md",
            LEGAL_COMMAND_REVIEW_CONTRACT,
        ),
        (
            "bundled/legal/commands/triage-nda.md",
            LEGAL_COMMAND_TRIAGE_NDA,
        ),
        (
            "bundled/legal/commands/vendor-check.md",
            LEGAL_COMMAND_VENDOR_CHECK,
        ),
    ];

    let commands = command_assets
        .into_iter()
        .filter_map(|(path, content)| {
            if content.trim().is_empty() {
                return None;
            }
            Some(CommandFile::from_markdown(path, plugin_id.clone(), content))
        })
        .collect::<Vec<_>>();

    if skills.is_empty() {
        errors.push("No bundled legal skills were embedded".to_string());
    }
    if commands.is_empty() {
        errors.push("No bundled legal commands were embedded".to_string());
    }

    let status = if errors.is_empty() {
        PluginStatus::Inactive
    } else {
        PluginStatus::Error
    };

    Plugin {
        id: plugin_id,
        name: manifest.name,
        version: manifest.version,
        description: manifest.description,
        path: PathBuf::from("bundled/legal"),
        enabled: false,
        skills,
        commands,
        mcp_servers,
        local_config: Some(DEFAULT_LEGAL_LOCAL_CONFIG.to_string()),
        status,
        errors,
    }
}

pub fn load_all_bundled_plugins() -> Vec<Plugin> {
    let legal = load_bundled_legal_plugin();

    // If no usable bundled content exists, avoid injecting a broken placeholder plugin.
    if legal.skills.is_empty() && legal.commands.is_empty() && legal.mcp_servers.is_empty() {
        return Vec::new();
    }

    vec![legal]
}
