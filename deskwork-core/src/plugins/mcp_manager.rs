use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value as JsonValue;
use tokio::time::timeout;

use serdes_ai_mcp::{CallToolResult, McpClient, McpTool};

use crate::plugins::types::McpServerEntry;

const MCP_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const MCP_LIST_TOOLS_TIMEOUT: Duration = Duration::from_secs(10);
const MCP_CALL_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct NamespacedMcpTool {
    pub tool_key: String,
    pub server_name: String,
    pub tool_name: String,
    pub description: Option<String>,
    pub input_schema: JsonValue,
}

#[derive(Default)]
pub struct PluginMcpManager {
    clients: HashMap<String, Arc<McpClient>>,
    tools: HashMap<String, NamespacedMcpTool>,
    unavailable: HashMap<String, String>,
}

impl std::fmt::Debug for PluginMcpManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PluginMcpManager")
            .field("client_count", &self.clients.len())
            .field("tool_count", &self.tools.len())
            .field("unavailable", &self.unavailable)
            .finish()
    }
}

impl PluginMcpManager {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn connect_from_configs(configs: &HashMap<String, McpServerEntry>) -> Self {
        let mut manager = Self::new();

        for (server_name, entry) in configs {
            let connect_result = connect_client(entry).await;
            let client = match connect_result {
                Ok(client) => Arc::new(client),
                Err(err) => {
                    manager.unavailable.insert(server_name.clone(), err);
                    continue;
                }
            };

            let tools = match timeout(MCP_LIST_TOOLS_TIMEOUT, client.list_tools()).await {
                Ok(Ok(tools)) => tools,
                Ok(Err(err)) => {
                    manager.unavailable.insert(
                        server_name.clone(),
                        format!("Failed to list MCP tools: {err}"),
                    );
                    continue;
                }
                Err(_) => {
                    manager.unavailable.insert(
                        server_name.clone(),
                        format!(
                            "Timeout listing MCP tools after {}s",
                            MCP_LIST_TOOLS_TIMEOUT.as_secs()
                        ),
                    );
                    continue;
                }
            };

            for tool in tools {
                manager.register_tool(server_name, &tool);
            }

            manager.clients.insert(server_name.clone(), client);
        }

        manager
    }

    pub fn list_all_tools(&self) -> HashMap<String, NamespacedMcpTool> {
        self.tools.clone()
    }

    pub fn unavailable_connectors(&self) -> &HashMap<String, String> {
        &self.unavailable
    }

    pub async fn call_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        args: JsonValue,
    ) -> Result<CallToolResult, String> {
        let Some(client) = self.clients.get(server_name) else {
            if let Some(reason) = self.unavailable.get(server_name) {
                return Err(format!("MCP server `{server_name}` unavailable: {reason}"));
            }
            return Err(format!("MCP server `{server_name}` is not connected"));
        };

        match timeout(MCP_CALL_TIMEOUT, client.call_tool(tool_name, args)).await {
            Ok(Ok(result)) => Ok(result),
            Ok(Err(err)) => Err(format!(
                "MCP tool call failed for `{server_name}:{tool_name}`: {err}"
            )),
            Err(_) => Err(format!(
                "Timeout calling MCP tool `{server_name}:{tool_name}` after {}s",
                MCP_CALL_TIMEOUT.as_secs()
            )),
        }
    }

    fn register_tool(&mut self, server_name: &str, tool: &McpTool) {
        let key = mcp_tool_key(server_name, &tool.name);
        self.tools.insert(
            key.clone(),
            NamespacedMcpTool {
                tool_key: key,
                server_name: server_name.to_string(),
                tool_name: tool.name.clone(),
                description: tool.description.clone(),
                input_schema: tool.input_schema.clone(),
            },
        );
    }
}

pub fn mcp_tool_key(server_name: &str, tool_name: &str) -> String {
    let server = sanitize_identifier(server_name);
    let tool = sanitize_identifier(tool_name);
    format!("mcp__{server}__{tool}")
}

fn sanitize_identifier(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push('_');
        }
    }
    out.trim_matches('_').to_string()
}

async fn connect_client(entry: &McpServerEntry) -> Result<McpClient, String> {
    match entry.r#type.to_ascii_lowercase().as_str() {
        "http" => {
            let url = entry
                .url
                .as_deref()
                .filter(|u| !u.trim().is_empty())
                .ok_or_else(|| "Missing `url` for MCP connector type `http`".to_string())?;

            let client = McpClient::http(url);
            match timeout(MCP_CONNECT_TIMEOUT, client.initialize()).await {
                Ok(Ok(_)) => Ok(client),
                Ok(Err(err)) => Err(format!("Failed to initialize HTTP MCP connector: {err}")),
                Err(_) => Err(format!(
                    "Timeout initializing HTTP MCP connector after {}s",
                    MCP_CONNECT_TIMEOUT.as_secs()
                )),
            }
        }
        "stdio" => {
            let command = entry
                .command
                .as_deref()
                .filter(|c| !c.trim().is_empty())
                .ok_or_else(|| "Missing `command` for MCP connector type `stdio`".to_string())?;

            let arg_storage = entry.args.clone().unwrap_or_default();
            let arg_refs = arg_storage.iter().map(String::as_str).collect::<Vec<_>>();

            let client =
                match timeout(MCP_CONNECT_TIMEOUT, McpClient::stdio(command, &arg_refs)).await {
                    Ok(Ok(client)) => client,
                    Ok(Err(err)) => {
                        return Err(format!("Failed to spawn stdio MCP connector: {err}"));
                    }
                    Err(_) => {
                        return Err(format!(
                            "Timeout spawning stdio MCP connector after {}s",
                            MCP_CONNECT_TIMEOUT.as_secs()
                        ));
                    }
                };

            match timeout(MCP_CONNECT_TIMEOUT, client.initialize()).await {
                Ok(Ok(_)) => Ok(client),
                Ok(Err(err)) => Err(format!("Failed to initialize stdio MCP connector: {err}")),
                Err(_) => Err(format!(
                    "Timeout initializing stdio MCP connector after {}s",
                    MCP_CONNECT_TIMEOUT.as_secs()
                )),
            }
        }
        other => Err(format!(
            "Unsupported MCP connector type `{other}`. Expected `http` or `stdio`"
        )),
    }
}
