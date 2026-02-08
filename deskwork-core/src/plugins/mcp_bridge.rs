use std::collections::HashMap;

type McpMap = HashMap<String, McpServerEntry>;

use crate::plugins::types::{McpServerEntry, Plugin};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnavailableConnector {
    pub namespaced_name: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default)]
pub struct McpBridgeResult {
    pub configs: McpMap,
    pub unavailable: Vec<UnavailableConnector>,
}

pub fn build_namespaced_mcp_map<'a>(
    plugins: impl IntoIterator<Item = &'a Plugin>,
) -> McpBridgeResult {
    let mut result = McpBridgeResult::default();

    for plugin in plugins {
        if !plugin.enabled {
            continue;
        }

        for (connector_name, entry) in &plugin.mcp_servers {
            let namespaced = format!("{}:{connector_name}", plugin.id);
            match resolve_entry_placeholders(entry) {
                Ok(resolved) => {
                    result.configs.insert(namespaced, resolved);
                }
                Err(reason) => {
                    result.unavailable.push(UnavailableConnector {
                        namespaced_name: namespaced,
                        reason,
                    });
                }
            }
        }
    }

    result
}

pub fn resolve_entry_placeholders(entry: &McpServerEntry) -> Result<McpServerEntry, String> {
    let mut missing = Vec::new();

    let url = entry
        .url
        .as_deref()
        .map(|v| resolve_env_placeholders(v, &mut missing));
    let command = entry
        .command
        .as_deref()
        .map(|v| resolve_env_placeholders(v, &mut missing));
    let args = entry.args.as_ref().map(|items| {
        items
            .iter()
            .map(|item| resolve_env_placeholders(item, &mut missing))
            .collect::<Vec<_>>()
    });
    let env = entry.env.as_ref().map(|map| {
        map.iter()
            .map(|(k, v)| (k.clone(), resolve_env_placeholders(v, &mut missing)))
            .collect::<HashMap<_, _>>()
    });

    if !missing.is_empty() {
        missing.sort();
        missing.dedup();
        return Err(format!(
            "Connector is unavailable because these environment variables are missing: {}",
            missing.join(", ")
        ));
    }

    let resolved = McpServerEntry {
        r#type: entry.r#type.clone(),
        url,
        command,
        args,
        env,
    };

    validate_mcp_entry(&resolved)?;

    Ok(resolved)
}

fn validate_mcp_entry(entry: &McpServerEntry) -> Result<(), String> {
    let kind = entry.r#type.trim().to_ascii_lowercase();

    match kind.as_str() {
        "http" => {
            if entry
                .url
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .is_empty()
            {
                return Err("Connector type `http` requires a non-empty `url` field".to_string());
            }
        }
        "stdio" => {
            if entry
                .command
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .is_empty()
            {
                return Err(
                    "Connector type `stdio` requires a non-empty `command` field".to_string(),
                );
            }
        }
        other => {
            return Err(format!(
                "Unsupported MCP connector type `{other}`. Expected `http` or `stdio`"
            ));
        }
    }

    Ok(())
}

pub fn resolve_env_placeholders(input: &str, missing: &mut Vec<String>) -> String {
    let bytes = input.as_bytes();
    let mut i = 0;
    let mut out = String::with_capacity(input.len());

    while i < bytes.len() {
        if bytes[i] == b'$' && i + 1 < bytes.len() && bytes[i + 1] == b'{' {
            let start = i + 2;
            if let Some(end_rel) = input[start..].find('}') {
                let end = start + end_rel;
                let var_name = &input[start..end];
                match std::env::var(var_name) {
                    Ok(value) => out.push_str(&value),
                    Err(_) => {
                        missing.push(var_name.to_string());
                    }
                }
                i = end + 1;
                continue;
            }
        }

        out.push(bytes[i] as char);
        i += 1;
    }

    out
}
