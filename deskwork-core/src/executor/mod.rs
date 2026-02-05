//! Agent executor for running Claude with tools.
//!
//! This module provides the core execution layer that connects Claude to our tools
//! and streams events back to the GUI.

use std::sync::Arc;

use tokio::sync::mpsc;
use tracing::{debug, error, info};

use serdes_ai_agent::{agent, AgentStream, AgentStreamEvent, RunOptions, ToolExecutor};
use serdes_ai_models::Model;
use serdes_ai_core::ModelRequest;
use serdes_ai_core::messages::{ImageMediaType, UserContent, UserContentPart};
use serdes_ai_tools::{RunContext as ToolRunContext, Tool, ToolError, ToolReturn};

use crate::config::Settings;
use crate::tools::ToolRegistry;

// =============================================================================
// Events
// =============================================================================

/// Events sent from executor to GUI.
///
/// These events represent the streaming output from Claude, including text,
/// thinking, tool calls, and completion information.
#[derive(Debug, Clone)]
pub enum ExecutorEvent {
    /// Run has started.
    RunStart { run_id: String },

    /// Streaming text from assistant.
    TextDelta(String),

    /// Extended thinking text (when thinking mode is enabled).
    ThinkingDelta(String),

    /// Tool call started.
    ToolCallStart {
        id: Option<String>,
        name: String,
    },

    /// Tool call arguments streaming.
    ToolCallDelta {
        id: Option<String>,
        delta: String,
    },

    /// Tool call completed (arguments fully received).
    ToolCallComplete {
        id: Option<String>,
        name: String,
    },

    /// Tool execution result.
    ToolResult {
        id: Option<String>,
        name: String,
        result: String,
        success: bool,
    },

    /// Generation complete.
    Done {
        input_tokens: u32,
        output_tokens: u32,
    },

    /// Error occurred.
    Error(String),

    /// Run was cancelled.
    Cancelled,
}

// =============================================================================
// Channel Types
// =============================================================================

/// Sender for executor events.
pub type EventSender = mpsc::UnboundedSender<ExecutorEvent>;

/// Receiver for executor events.
pub type EventReceiver = mpsc::UnboundedReceiver<ExecutorEvent>;

/// Create an event channel for streaming executor events.
pub fn event_channel() -> (EventSender, EventReceiver) {
    mpsc::unbounded_channel()
}

// =============================================================================
// Tool Wrapper
// =============================================================================

/// Wrapper that adapts a `serdes_ai_tools::Tool` to the agent's `ToolExecutor` trait.
///
/// This bridges our tool implementations (which use `RunContext<()>`) to the
/// agent system's generic deps system.
struct ToolWrapper {
    tool: Arc<dyn Tool>,
}

impl ToolWrapper {
    fn new(tool: Arc<dyn Tool>) -> Self {
        Self { tool }
    }
}

#[async_trait::async_trait]
impl<Deps: Send + Sync> ToolExecutor<Deps> for ToolWrapper {
    async fn execute(
        &self,
        args: serde_json::Value,
        ctx: &serdes_ai_agent::RunContext<Deps>,
    ) -> Result<ToolReturn, ToolError> {
        // Create a tool context without deps (our tools don't use deps)
        let tool_ctx = ToolRunContext::minimal(&ctx.model_name)
            .with_run_id(&ctx.run_id)
            .with_tool_context(
                self.tool.definition().name(),
                ctx.tool_call_id.clone(),
            );

        // Call the tool
        self.tool
            .call(&tool_ctx, args)
            .await
            .map_err(|e| ToolError::execution_failed(format!("{}: {}", self.tool.definition().name(), e)))
    }
}

// =============================================================================
// Agent Execution
// =============================================================================

/// Run the agent with streaming output.
///
/// This spawns a background task that streams events back via the channel.
/// Returns a handle that can be used to abort the generation.
///
/// # Arguments
///
/// * `access_token` - OAuth access token from Claude authentication
/// * `model_name` - The model ID to use (e.g., "claude-sonnet-4-20250514")
/// * `settings` - User settings (temperature, thinking mode, etc.)
/// * `system_prompt` - System prompt for the assistant
/// * `user_input` - The user's message
/// * `message_history` - Previous conversation messages (optional)
/// * `event_sender` - Channel to send streaming events
///
/// # Example
///
/// ```ignore
/// use deskwork_core::executor::{run_agent, event_channel, ExecutorEvent};
///
/// let (tx, mut rx) = event_channel();
///
/// let handle = run_agent(
///     access_token,
///     "claude-sonnet-4-20250514".to_string(),
///     settings,
///     system_prompt,
///     "Help me write a function".to_string(),
///     vec![],
///     tx,
/// ).await;
///
/// // Process events
/// while let Some(event) = rx.recv().await {
///     match event {
///         ExecutorEvent::TextDelta(text) => print!("{}", text),
///         ExecutorEvent::Done { .. } => break,
///         _ => {}
///     }
/// }
/// ```

/// Image data with media type for multimodal requests.
pub struct ImageData {
    pub data: Vec<u8>,
    pub media_type: ImageMediaType,
}

pub async fn run_agent(
    access_token: String,
    model_name: String,
    settings: Settings,
    system_prompt: String,
    user_input: String,
    images: Vec<ImageData>,
    message_history: Vec<ModelRequest>,
    event_sender: EventSender,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        info!("Starting agent execution");

        // Create the model using OAuth token
        let model = crate::claude::create_model(&model_name, &access_token, &settings);
        debug!(model = %model.name(), "Created model");

        // Create tools registry and get tools
        let registry = ToolRegistry::with_defaults();

        // Claude Code OAuth uses hardcoded settings (like workitforme)
        // - Temperature: 1.0 (required for extended thinking)
        // - Max tokens: 30000 (Claude Code OAuth default)
        let mut builder = agent(model)
            .system_prompt(&system_prompt)
            .temperature(1.0)
            .max_tokens(30000);

        // Add each tool by wrapping it
        for tool in registry.tools() {
            let definition = tool.definition();
            let wrapper = ToolWrapper::new(Arc::clone(tool));
            builder = builder.tool_with_executor(definition, wrapper);
        }

        let agent = builder.build();
        debug!(tools = registry.len(), "Agent built with tools");

        // Prepare run options with history
        let options = if message_history.is_empty() {
            RunOptions::default()
        } else {
            RunOptions::default().message_history(message_history)
        };

        // Build user content (text + optional images)
        let user_content = if images.is_empty() {
            UserContent::text(&user_input)
        } else {
            let mut parts = Vec::new();
            if !user_input.is_empty() {
                parts.push(UserContentPart::text(&user_input));
            }
            for img in &images {
                parts.push(UserContentPart::image_binary(
                    img.data.clone(),
                    img.media_type,
                ));
            }
            UserContent::parts(parts)
        };

        info!(
            "Sending request with {} images",
            images.len()
        );

        // Run with streaming
        match AgentStream::new(&agent, user_content, (), options).await {
            Ok(stream) => {
                process_stream(stream, event_sender).await;
            }
            Err(e) => {
                error!(error = %e, "Failed to start agent stream");
                let _ = event_sender.send(ExecutorEvent::Error(e.to_string()));
            }
        }
    })
}

/// Process the agent stream and forward events to the GUI.
async fn process_stream(mut stream: AgentStream, sender: EventSender) {
    use futures::StreamExt;

    while let Some(result) = stream.next().await {
        match result {
            Ok(event) => {
                let executor_event = convert_event(event);
                if let Some(ev) = executor_event {
                    let is_done = matches!(ev, ExecutorEvent::Done { .. } | ExecutorEvent::Error(_));
                    if sender.send(ev).is_err() {
                        debug!("Event receiver dropped, stopping stream");
                        break;
                    }
                    if is_done {
                        break;
                    }
                }
            }
            Err(e) => {
                error!(error = %e, "Stream error");
                let _ = sender.send(ExecutorEvent::Error(e.to_string()));
                break;
            }
        }
    }

    debug!("Stream processing complete");
}

/// Convert an AgentStreamEvent to our ExecutorEvent.
fn convert_event(event: AgentStreamEvent) -> Option<ExecutorEvent> {
    match event {
        AgentStreamEvent::RunStart { run_id } => Some(ExecutorEvent::RunStart { run_id }),

        AgentStreamEvent::TextDelta { text } => Some(ExecutorEvent::TextDelta(text)),

        AgentStreamEvent::ThinkingDelta { text } => Some(ExecutorEvent::ThinkingDelta(text)),

        AgentStreamEvent::ToolCallStart {
            tool_name,
            tool_call_id,
        } => Some(ExecutorEvent::ToolCallStart {
            id: tool_call_id,
            name: tool_name,
        }),

        AgentStreamEvent::ToolCallDelta {
            delta,
            tool_call_id,
        } => Some(ExecutorEvent::ToolCallDelta {
            id: tool_call_id,
            delta,
        }),

        AgentStreamEvent::ToolCallComplete {
            tool_name,
            tool_call_id,
        } => Some(ExecutorEvent::ToolCallComplete {
            id: tool_call_id,
            name: tool_name,
        }),

        AgentStreamEvent::ToolExecuted {
            tool_name,
            tool_call_id,
            success,
            error,
        } => {
            let result = error.unwrap_or_else(|| "Success".to_string());
            Some(ExecutorEvent::ToolResult {
                id: tool_call_id,
                name: tool_name,
                result,
                success,
            })
        }

        AgentStreamEvent::RunComplete { .. } => {
            // We'll get usage from OutputReady or calculate it
            Some(ExecutorEvent::Done {
                input_tokens: 0,
                output_tokens: 0,
            })
        }

        AgentStreamEvent::Error { message } => Some(ExecutorEvent::Error(message)),

        AgentStreamEvent::Cancelled { .. } => Some(ExecutorEvent::Cancelled),

        // Ignore other events (RequestStart, ResponseComplete, ContextInfo, etc.)
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_channel() {
        let (tx, _rx) = event_channel();
        assert!(tx.send(ExecutorEvent::TextDelta("test".to_string())).is_ok());
    }

    #[test]
    fn test_convert_text_delta() {
        let event = AgentStreamEvent::TextDelta {
            text: "Hello".to_string(),
        };
        let result = convert_event(event);
        assert!(matches!(result, Some(ExecutorEvent::TextDelta(s)) if s == "Hello"));
    }

    #[test]
    fn test_convert_thinking_delta() {
        let event = AgentStreamEvent::ThinkingDelta {
            text: "Reasoning...".to_string(),
        };
        let result = convert_event(event);
        assert!(matches!(result, Some(ExecutorEvent::ThinkingDelta(s)) if s == "Reasoning..."));
    }

    #[test]
    fn test_convert_tool_start() {
        let event = AgentStreamEvent::ToolCallStart {
            tool_name: "read_file".to_string(),
            tool_call_id: Some("call_123".to_string()),
        };
        let result = convert_event(event);
        assert!(matches!(
            result,
            Some(ExecutorEvent::ToolCallStart { name, id }) 
            if name == "read_file" && id == Some("call_123".to_string())
        ));
    }

    #[test]
    fn test_convert_error() {
        let event = AgentStreamEvent::Error {
            message: "API error".to_string(),
        };
        let result = convert_event(event);
        assert!(matches!(result, Some(ExecutorEvent::Error(s)) if s == "API error"));
    }
}
