use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value as JsonValue;

use serdes_ai_mcp::ToolResultContent;
use serdes_ai_tools::{RunContext, Tool, ToolDefinition, ToolError, ToolResult, ToolReturn};

use crate::plugins::mcp_manager::{NamespacedMcpTool, PluginMcpManager};

#[derive(Debug, Clone)]
pub struct PluginMcpTool {
    manager: Arc<PluginMcpManager>,
    server_name: String,
    tool_name: String,
    definition: ToolDefinition,
}

impl PluginMcpTool {
    pub fn new(manager: Arc<PluginMcpManager>, meta: NamespacedMcpTool) -> Self {
        let description = meta.description.clone().unwrap_or_else(|| {
            format!(
                "MCP tool `{}` exposed by server `{}`",
                meta.tool_name, meta.server_name
            )
        });

        let definition = ToolDefinition::new(meta.tool_key, description)
            .with_parameters(meta.input_schema.clone());

        Self {
            manager,
            server_name: meta.server_name,
            tool_name: meta.tool_name,
            definition,
        }
    }
}

#[async_trait]
impl Tool for PluginMcpTool {
    fn definition(&self) -> ToolDefinition {
        self.definition.clone()
    }

    async fn call(&self, _ctx: &RunContext, args: JsonValue) -> ToolResult {
        let result = self
            .manager
            .call_tool(&self.server_name, &self.tool_name, args)
            .await
            .map_err(ToolError::execution_failed)?;

        if result.is_error {
            return Err(ToolError::execution_failed(
                extract_text_result(&result).unwrap_or_else(|| {
                    format!("MCP tool `{}` returned an error", self.definition.name())
                }),
            ));
        }

        if let Some(text) = extract_text_result(&result) {
            return Ok(ToolReturn::text(text));
        }

        match serde_json::to_value(&result) {
            Ok(json) => Ok(ToolReturn::json(json)),
            Err(err) => Ok(ToolReturn::text(format!(
                "MCP tool `{}` completed (non-JSON result fallback): {}",
                self.definition.name(),
                err
            ))),
        }
    }
}

fn extract_text_result(result: &serdes_ai_mcp::CallToolResult) -> Option<String> {
    let texts = result
        .content
        .iter()
        .filter_map(|content| match content {
            ToolResultContent::Text { text } => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();

    if texts.is_empty() {
        None
    } else {
        Some(texts.join("\n"))
    }
}
