//! Main application state and update loop.

use eframe::egui;
use tokio::runtime::Runtime;
use tracing::{debug, error, info, warn};

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use deskwork_core::{
    build_system_prompt, event_channel, run_agent, ClaudeCodeAuth, Database, EventReceiver,
    ExecutorEvent, ImageData, ImageMediaType, PluginRuntime, RunAgentArgs, Settings,
};

use crate::ui;
use crate::ui::attachments::{self, PendingImage};

// =============================================================================
// Message Types
// =============================================================================

/// A message in the chat.
#[derive(Debug, Clone)]
pub struct Message {
    pub role: MessageRole,
    /// User message text content (for User role).
    pub content: String,
    /// Ordered content blocks (for Assistant role). Preserves chronological order
    /// of thinking, tool calls, and text responses.
    pub blocks: Vec<ContentBlock>,
    #[allow(dead_code)]
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Message {
    pub fn user(content: String) -> Self {
        Self {
            role: MessageRole::User,
            content,
            blocks: Vec::new(),
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn assistant() -> Self {
        Self {
            role: MessageRole::Assistant,
            content: String::new(),
            blocks: Vec::new(),
            timestamp: chrono::Utc::now(),
        }
    }
}

/// Message sender.
#[derive(Debug, Clone, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
}

/// A tool call within a message.
#[derive(Debug, Clone)]
pub struct ToolCall {
    #[allow(dead_code)]
    pub id: Option<String>,
    pub name: String,
    pub arguments: String,
    pub result: Option<String>,
    pub success: bool,
    #[allow(dead_code)]
    pub collapsed: bool,
}

impl ToolCall {
    pub fn new(id: Option<String>, name: String) -> Self {
        Self {
            id,
            name,
            arguments: String::new(),
            result: None,
            success: true,
            collapsed: true,
        }
    }
}

/// A content block within an assistant message, preserving chronological order.
#[derive(Debug, Clone)]
pub enum ContentBlock {
    /// Extended thinking / reasoning from Claude.
    Thinking(String),
    /// A tool call (with arguments and optional result).
    ToolUse(ToolCall),
    /// Text response from the assistant.
    Text(String),
}

// =============================================================================
// Authentication State
// =============================================================================

/// OAuth authentication state.
#[derive(Debug, Clone, PartialEq)]
pub enum AuthState {
    /// Not authenticated.
    NotAuthenticated,
    /// Authentication in progress.
    Authenticating,
    /// Authenticated with access token.
    Authenticated,
    /// Auth error.
    Error(String),
}

// =============================================================================
// Application State
// =============================================================================

/// UI state for an external tool.
#[derive(Debug, Clone)]
pub struct ToolStatusUi {
    pub is_installed: bool,
    pub version: Option<String>,
    pub is_installing: bool,
    pub install_progress: u8,
    pub is_supported: bool,
}

/// Tracks the kind of block currently being streamed, so we know when to start a new block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamingBlockKind {
    None,
    Thinking,
    Text,
    ToolUse,
}

/// Main application state.
pub struct DeskworkApp {
    /// Tokio runtime for async operations.
    pub runtime: Runtime,

    /// Database connection.
    pub db: Database,

    /// User settings.
    pub settings: Settings,

    /// Plugin runtime and registry state.
    pub plugin_runtime: PluginRuntime,

    /// Skills context for system prompt injection.
    pub skills_context: Option<deskwork_core::SkillsContext>,

    // -------------------------------------------------------------------------
    // Authentication
    // -------------------------------------------------------------------------
    /// Authentication state.
    pub auth_state: AuthState,

    /// Available models (fetched after auth).
    pub available_models: Vec<String>,

    /// Whether we're currently fetching models.
    pub fetching_models: bool,

    // -------------------------------------------------------------------------
    // Chat State
    // -------------------------------------------------------------------------
    /// Message history.
    pub messages: Vec<Message>,

    /// Current input text.
    pub input: String,

    /// Whether we're currently generating a response.
    pub is_generating: bool,

    // -------------------------------------------------------------------------
    // Streaming State
    // -------------------------------------------------------------------------
    /// Ordered content blocks being streamed for the current assistant response.
    pub current_blocks: Vec<ContentBlock>,

    /// What kind of block is currently being appended to.
    streaming_block_kind: StreamingBlockKind,

    /// Event receiver for streaming.
    pub event_rx: Option<EventReceiver>,

    /// Handle to the generation task.
    pub generation_handle: Option<tokio::task::JoinHandle<()>>,

    // -------------------------------------------------------------------------
    // UI State
    // -------------------------------------------------------------------------
    /// Show settings dialog.
    pub show_settings: bool,

    /// Currently selected settings tab.
    pub settings_tab: crate::ui::settings::SettingsTab,

    /// Current working directory.
    pub working_dir: Option<std::path::PathBuf>,

    /// Whether to scroll to bottom on next frame.
    pub scroll_to_bottom: bool,

    /// Status message.
    pub status_message: Option<(String, chrono::DateTime<chrono::Utc>)>,

    /// Pending auth result receiver.
    auth_result_rx: Option<tokio::sync::oneshot::Receiver<Result<(), String>>>,

    /// Pending models result receiver.
    models_result_rx: Option<tokio::sync::oneshot::Receiver<Result<Vec<String>, String>>>,

    /// Pending folder selection result receiver.
    folder_result_rx: Option<tokio::sync::oneshot::Receiver<Option<std::path::PathBuf>>>,

    /// External tool installation statuses.
    pub tool_statuses: std::collections::HashMap<deskwork_core::ExternalToolId, ToolStatusUi>,

    /// Channel for receiving tool status refresh results.
    tool_status_rx:
        Option<tokio::sync::oneshot::Receiver<Vec<(deskwork_core::ExternalToolId, ToolStatusUi)>>>,

    /// Channels for receiving tool install progress (tool_id -> progress_percent).
    tool_install_progress_rx: Vec<(
        deskwork_core::ExternalToolId,
        tokio::sync::mpsc::Receiver<u8>,
    )>,

    /// Channels for receiving tool install completion.
    tool_install_result_rx: Vec<(
        deskwork_core::ExternalToolId,
        tokio::sync::oneshot::Receiver<Result<(), String>>,
    )>,

    /// Channels for receiving tool uninstall completion.
    tool_uninstall_result_rx: Vec<(
        deskwork_core::ExternalToolId,
        tokio::sync::oneshot::Receiver<Result<(), String>>,
    )>,

    /// Pending image attachments.
    pub pending_attachments: Vec<PendingImage>,
}

fn default_plugins_dir() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join(".deskwork").join("plugins");
    }

    PathBuf::from(".deskwork/plugins")
}

fn resolve_plugins_dir(settings: &Settings) -> PathBuf {
    settings
        .plugins_dir
        .as_ref()
        .filter(|p| !p.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(default_plugins_dir)
}

fn parse_inline_command_inputs(input: &str) -> HashMap<String, String> {
    let Some((_, args)) = input.trim().split_once(' ') else {
        return HashMap::new();
    };

    args.split_whitespace()
        .filter_map(|item| item.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .filter(|(k, _)| !k.is_empty())
        .collect()
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    if !src.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Source is not a directory: {}", src.display()),
        ));
    }

    fs::create_dir_all(dst)?;

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let target_path = dst.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &target_path)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), target_path)?;
        }
    }

    Ok(())
}

impl DeskworkApp {
    /// Create a new application instance.
    pub fn new(cc: &eframe::CreationContext<'_>, runtime: Runtime) -> Self {
        info!("Initializing DeskworkApp");

        // Open database
        let db = match Database::open() {
            Ok(db) => {
                if let Err(e) = db.migrate() {
                    error!("Failed to migrate database: {}", e);
                }
                db
            }
            Err(e) => {
                error!("Failed to open database: {}", e);
                // Create in-memory fallback
                Database::open_at(PathBuf::from(":memory:"))
                    .expect("Failed to create in-memory database")
            }
        };

        // Load settings
        let mut settings = Settings::load(&db);
        settings.validate();
        debug!(?settings, "Loaded settings");

        // Extract bundled skills if needed
        match deskwork_core::extract_skills_if_needed() {
            Ok(skills_dir) => {
                info!("Skills available at: {}", skills_dir.display());
            }
            Err(e) => {
                error!("Failed to extract skills bundle: {}", e);
            }
        }

        // Build skills context (best-effort, don't block on failures)
        let skills_context = Some(deskwork_core::SkillsContext::build());

        // Initialize plugin runtime
        let plugins_dir = resolve_plugins_dir(&settings);
        let mut plugin_runtime = PluginRuntime::new(&plugins_dir, &settings.plugins_enabled);
        plugin_runtime.set_context_budget(settings.plugin_context_token_budget as usize);
        plugin_runtime.load();

        // Restore working directory from settings
        let working_dir = settings.working_directory.as_ref().map(PathBuf::from);

        // Check if already authenticated
        let auth_state = {
            let auth = ClaudeCodeAuth::new(&db);
            if auth.is_authenticated() {
                AuthState::Authenticated
            } else {
                AuthState::NotAuthenticated
            }
        };

        // Get available models from settings
        let available_models = settings.available_models.clone();

        // Apply theme
        let visuals = match settings.theme {
            deskwork_core::Theme::Dark => egui::Visuals::dark(),
            deskwork_core::Theme::Light => egui::Visuals::light(),
        };
        cc.egui_ctx.set_visuals(visuals);

        // Configure fonts for better readability
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        cc.egui_ctx.set_style(style);

        Self {
            runtime,
            db,
            settings,
            plugin_runtime,
            skills_context,
            auth_state,
            available_models,
            fetching_models: false,
            messages: Vec::new(),
            input: String::new(),
            is_generating: false,
            current_blocks: Vec::new(),
            streaming_block_kind: StreamingBlockKind::None,
            event_rx: None,
            generation_handle: None,
            show_settings: false,
            settings_tab: Default::default(),
            working_dir,
            scroll_to_bottom: false,
            status_message: None,
            auth_result_rx: None,
            models_result_rx: None,
            folder_result_rx: None,
            tool_statuses: std::collections::HashMap::new(),
            tool_status_rx: None,
            tool_install_progress_rx: Vec::new(),
            tool_install_result_rx: Vec::new(),
            tool_uninstall_result_rx: Vec::new(),
            pending_attachments: Vec::new(),
        }
    }

    /// Check if authenticated.
    pub fn is_authenticated(&self) -> bool {
        self.auth_state == AuthState::Authenticated
    }

    /// Start the OAuth authentication flow.
    pub fn start_auth(&mut self) {
        if self.auth_state == AuthState::Authenticating {
            return;
        }

        info!("Starting OAuth authentication");
        self.auth_state = AuthState::Authenticating;
        self.set_status("Opening browser for authentication...");

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.auth_result_rx = Some(rx);

        // We need to clone db path since we can't send Database across threads
        let db_path = self.db.path().to_path_buf();

        self.runtime.spawn(async move {
            match deskwork_core::run_claude_code_auth(db_path).await {
                Ok(_tokens) => {
                    let _ = tx.send(Ok(()));
                }
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                }
            }
        });
    }

    /// Check for auth completion.
    fn check_auth_completion(&mut self) {
        if let Some(mut rx) = self.auth_result_rx.take() {
            match rx.try_recv() {
                Ok(Ok(())) => {
                    info!("Authentication successful");
                    self.auth_state = AuthState::Authenticated;
                    self.set_status("Signed in successfully!");
                    // Fetch available models
                    self.fetch_models();
                }
                Ok(Err(e)) => {
                    error!("Authentication failed: {}", e);
                    self.auth_state = AuthState::Error(e.clone());
                    self.set_status(&format!("Auth failed: {}", e));
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                    // Still waiting
                    self.auth_result_rx = Some(rx);
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    warn!("Auth channel closed unexpectedly");
                    self.auth_state = AuthState::NotAuthenticated;
                }
            }
        }
    }

    /// Fetch available models from the API.
    pub fn fetch_models(&mut self) {
        if self.fetching_models || !self.is_authenticated() {
            return;
        }

        info!("Fetching available models");
        self.fetching_models = true;

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.models_result_rx = Some(rx);

        let db_path = self.db.path().to_path_buf();

        self.runtime.spawn(async move {
            // Get token using blocking call in separate block
            let token_result = {
                let db = match Database::open_at(db_path.clone()) {
                    Ok(db) => db,
                    Err(e) => {
                        let _ = tx.send(Err(format!("Failed to open database: {}", e)));
                        return;
                    }
                };
                let auth = ClaudeCodeAuth::new(&db);
                auth.get_access_token()
            };

            match token_result {
                Ok(token) => match deskwork_core::fetch_claude_models(&token).await {
                    Ok(models) => {
                        let filtered = deskwork_core::filter_latest_models(models);

                        // Save models to database with prefix
                        let db = match Database::open_at(db_path) {
                            Ok(db) => db,
                            Err(e) => {
                                let _ = tx.send(Err(format!("Failed to open database: {}", e)));
                                return;
                            }
                        };

                        if let Err(e) = deskwork_core::save_claude_models_to_db(&db, &filtered) {
                            warn!("Failed to save models to database: {}", e);
                        }

                        // Return prefixed model names
                        let prefixed: Vec<String> = filtered
                            .iter()
                            .map(|m| format!("claude-code-{}", m))
                            .collect();
                        let _ = tx.send(Ok(prefixed));
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e.to_string()));
                    }
                },
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                }
            }
        });
    }

    /// Check for model fetch completion.
    fn check_models_completion(&mut self) {
        if let Some(mut rx) = self.models_result_rx.take() {
            match rx.try_recv() {
                Ok(Ok(models)) => {
                    info!("Fetched {} models", models.len());
                    self.available_models = models.clone();
                    self.settings.set_available_models(models);
                    self.fetching_models = false;
                    self.save_settings();
                }
                Ok(Err(e)) => {
                    error!("Failed to fetch models: {}", e);
                    self.fetching_models = false;
                    self.set_status(&format!("Failed to fetch models: {}", e));
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                    // Still waiting
                    self.models_result_rx = Some(rx);
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    warn!("Models channel closed unexpectedly");
                    self.fetching_models = false;
                }
            }
        }
    }

    /// Sign out (clear tokens).
    pub fn sign_out(&mut self) {
        info!("Signing out");
        let auth = ClaudeCodeAuth::new(&self.db);
        if let Err(e) = auth.sign_out() {
            error!("Failed to sign out: {}", e);
        }
        self.auth_state = AuthState::NotAuthenticated;
        self.available_models.clear();
        self.settings.available_models.clear();
        self.save_settings();
        self.set_status("Signed out");
    }

    /// Get access token, refreshing if needed.
    fn get_access_token(&self) -> Option<String> {
        let auth = ClaudeCodeAuth::new(&self.db);
        auth.get_access_token().ok()
    }

    /// Send the current input as a message.
    pub fn send_message(&mut self) {
        let raw_input = self.input.trim().to_string();
        if raw_input.is_empty() {
            return;
        }

        // Don't send if already generating
        if self.is_generating {
            self.set_status("Please wait for the current response to complete");
            return;
        }

        if !self.is_authenticated() {
            self.set_status("Please sign in first");
            return;
        }

        // Get access token
        let access_token = match self.get_access_token() {
            Some(token) => token,
            None => {
                self.set_status("Not authenticated. Please sign in.");
                self.auth_state = AuthState::NotAuthenticated;
                return;
            }
        };

        // Refresh plugin runtime so external local config edits are reflected immediately.
        self.plugin_runtime.reload();

        // Resolve slash commands into full command prompts.
        let mut agent_input = raw_input.clone();
        if raw_input.starts_with('/') {
            let user_inputs = parse_inline_command_inputs(&raw_input);
            match self
                .plugin_runtime
                .execute_command(&raw_input, &user_inputs)
            {
                Ok(prompt) => {
                    agent_input = prompt;
                }
                Err(err) => {
                    self.set_status(&format!("Slash command error: {err}"));
                    return;
                }
            }
        }

        info!(
            "Sending message: {}...",
            &raw_input[..raw_input.len().min(50)]
        );

        // Keep user-visible chat content as the original user input.
        self.messages.push(Message::user(raw_input.clone()));
        self.input.clear();

        // Reset streaming state
        self.current_blocks.clear();
        self.streaming_block_kind = StreamingBlockKind::None;
        self.is_generating = true;
        self.scroll_to_bottom = true;

        // Create event channel
        let (tx, rx) = event_channel();
        self.event_rx = Some(rx);

        self.plugin_runtime
            .set_context_budget(self.settings.plugin_context_token_budget as usize);
        let plugin_context = self.plugin_runtime.plugin_context();

        let skills_prompt = self
            .skills_context
            .as_ref()
            .map(|ctx| ctx.to_prompt_section(self.settings.working_directory.as_deref()));

        // Build system prompt
        let system_prompt = build_system_prompt(
            self.settings.extended_thinking,
            None,
            Some(plugin_context.prompt.as_str()),
            skills_prompt.as_deref(),
        );

        // Convert pending attachments to ImageData
        let images: Vec<ImageData> = self
            .pending_attachments
            .iter()
            .map(|img| ImageData {
                data: img.data.clone(),
                media_type: ImageMediaType::Png, // We always encode as PNG
            })
            .collect();

        // Spawn the agent
        let settings = self.settings.clone();
        let model_name = settings.model.clone();
        let plugin_mcp_configs = self.plugin_runtime.mcp_bridge_result().configs.clone();
        let handle = self.runtime.spawn(async move {
            let agent_handle = run_agent(RunAgentArgs {
                access_token,
                model_name,
                settings,
                system_prompt,
                user_input: agent_input,
                images,
                message_history: vec![], // TODO: Include message history
                plugin_mcp_configs,
                event_sender: tx,
            });

            // Wait for completion
            let _ = agent_handle.await;
        });

        // Clear attachments after sending
        self.pending_attachments.clear();

        self.generation_handle = Some(handle);
    }

    /// Stop the current generation.
    pub fn stop_generation(&mut self) {
        info!("Stopping generation");

        if let Some(handle) = self.generation_handle.take() {
            handle.abort();
        }

        self.finalize_response();
        self.is_generating = false;
        self.event_rx = None;
    }

    /// Process events from the agent stream.
    pub fn process_events(&mut self, ctx: &egui::Context) {
        if let Some(ref mut rx) = self.event_rx {
            // Process all available events
            while let Ok(event) = rx.try_recv() {
                match event {
                    ExecutorEvent::RunStart { run_id } => {
                        debug!(run_id, "Run started");
                    }

                    ExecutorEvent::TextDelta(text) => {
                        if self.streaming_block_kind != StreamingBlockKind::Text {
                            self.current_blocks.push(ContentBlock::Text(String::new()));
                            self.streaming_block_kind = StreamingBlockKind::Text;
                        }
                        if let Some(ContentBlock::Text(ref mut s)) = self.current_blocks.last_mut()
                        {
                            s.push_str(&text);
                        }
                        ctx.request_repaint();
                    }

                    ExecutorEvent::ThinkingDelta(text) => {
                        if self.streaming_block_kind != StreamingBlockKind::Thinking {
                            self.current_blocks
                                .push(ContentBlock::Thinking(String::new()));
                            self.streaming_block_kind = StreamingBlockKind::Thinking;
                        }
                        if let Some(ContentBlock::Thinking(ref mut s)) =
                            self.current_blocks.last_mut()
                        {
                            s.push_str(&text);
                        }
                        ctx.request_repaint();
                    }

                    ExecutorEvent::ToolCallStart { id, name } => {
                        debug!(name, "Tool call started");
                        self.current_blocks
                            .push(ContentBlock::ToolUse(ToolCall::new(id, name)));
                        self.streaming_block_kind = StreamingBlockKind::ToolUse;
                        ctx.request_repaint();
                    }

                    ExecutorEvent::ToolCallDelta { id: _, delta } => {
                        // Find the last ToolUse block and append to its arguments
                        if let Some(ContentBlock::ToolUse(ref mut tc)) = self
                            .current_blocks
                            .iter_mut()
                            .rev()
                            .find(|b| matches!(b, ContentBlock::ToolUse(_)))
                        {
                            tc.arguments.push_str(&delta);
                        }
                        ctx.request_repaint();
                    }

                    ExecutorEvent::ToolCallComplete { id: _, name } => {
                        debug!(name, "Tool call complete");
                    }

                    ExecutorEvent::ToolResult {
                        id: _,
                        name,
                        result,
                        success,
                    } => {
                        debug!(name, success, "Tool result");
                        // Find the last ToolUse block and set its result
                        if let Some(ContentBlock::ToolUse(ref mut tc)) = self
                            .current_blocks
                            .iter_mut()
                            .rev()
                            .find(|b| matches!(b, ContentBlock::ToolUse(_)))
                        {
                            tc.result = Some(result);
                            tc.success = success;
                        }
                        ctx.request_repaint();
                    }

                    ExecutorEvent::Done {
                        input_tokens,
                        output_tokens,
                    } => {
                        info!(input_tokens, output_tokens, "Generation complete");
                        self.finalize_response();
                        self.is_generating = false;
                        self.event_rx = None;
                        ctx.request_repaint();
                        break;
                    }

                    ExecutorEvent::Error(msg) => {
                        error!(error = %msg, "Agent error");
                        self.set_status(&format!("Error: {}", msg));
                        self.finalize_response();
                        self.is_generating = false;
                        self.event_rx = None;
                        ctx.request_repaint();
                        break;
                    }

                    ExecutorEvent::Cancelled => {
                        info!("Generation cancelled");
                        self.finalize_response();
                        self.is_generating = false;
                        self.event_rx = None;
                        ctx.request_repaint();
                        break;
                    }
                }
            }
        }
    }

    /// Finalize the current response into a message.
    fn finalize_response(&mut self) {
        if self.current_blocks.is_empty() {
            return;
        }

        let mut message = Message::assistant();
        message.blocks = std::mem::take(&mut self.current_blocks);
        self.streaming_block_kind = StreamingBlockKind::None;
        self.messages.push(message);
        self.scroll_to_bottom = true;
    }

    /// Set a status message.
    pub fn set_status(&mut self, msg: &str) {
        self.status_message = Some((msg.to_string(), chrono::Utc::now()));
    }

    /// Clear old status messages.
    pub fn clear_old_status(&mut self) {
        if let Some((_, time)) = &self.status_message {
            if chrono::Utc::now() - *time > chrono::Duration::seconds(5) {
                self.status_message = None;
            }
        }
    }

    /// Save settings to the database.
    pub fn save_settings(&mut self) {
        if let Err(e) = self.settings.save(&self.db) {
            error!("Failed to save settings: {}", e);
            self.set_status("Failed to save settings");
        } else {
            self.set_status("Settings saved");
        }
    }

    /// Reload plugins from bundled and local directories.
    pub fn reload_plugins(&mut self) {
        self.plugin_runtime.reload();
        self.set_status("Plugins reloaded");
    }

    /// Open a plugin's local config markdown in the default system editor.
    pub fn open_plugin_local_config(&mut self, plugin_id: &str) {
        let plugin = self
            .plugin_runtime
            .registry()
            .all_plugins()
            .iter()
            .find(|p| p.id == plugin_id)
            .cloned();

        let Some(plugin) = plugin else {
            self.set_status(&format!("Plugin `{plugin_id}` not found"));
            return;
        };

        let plugin_dir = self.plugin_directory().join(plugin_id);
        let local_config_path = plugin_dir.join(format!("{plugin_id}.local.md"));

        if !local_config_path.exists() {
            if let Err(err) = fs::create_dir_all(&plugin_dir) {
                self.set_status(&format!(
                    "Failed to create plugin directory for `{plugin_id}`: {err}"
                ));
                return;
            }

            let default_content = plugin.local_config.clone().unwrap_or_else(|| {
                format!("# {plugin_id}.local.md\n\n<!-- Local plugin configuration -->\n")
            });

            if let Err(err) = fs::write(&local_config_path, default_content) {
                self.set_status(&format!(
                    "Failed to create local config for `{plugin_id}`: {err}"
                ));
                return;
            }
        }

        let open_result = if cfg!(target_os = "macos") {
            std::process::Command::new("open")
                .arg(&local_config_path)
                .status()
        } else if cfg!(target_os = "windows") {
            std::process::Command::new("cmd")
                .args(["/C", "start", ""])
                .arg(&local_config_path)
                .status()
        } else {
            std::process::Command::new("xdg-open")
                .arg(&local_config_path)
                .status()
        };

        match open_result {
            Ok(status) if status.success() => {
                self.set_status(&format!("Opened local config for `{plugin_id}`"));
            }
            Ok(status) => {
                self.set_status(&format!(
                    "Failed to open local config for `{plugin_id}` (exit code: {:?})",
                    status.code()
                ));
            }
            Err(err) => {
                self.set_status(&format!("Failed to launch editor for `{plugin_id}`: {err}"));
            }
        }
    }

    /// Install (or replace) plugin folders from a git repository URL.
    pub fn install_plugin_from_git_url(&mut self, git_url: &str) {
        let url = git_url.trim();
        if url.is_empty() {
            self.set_status("Git URL is empty");
            return;
        }

        let temp_clone_dir = std::env::temp_dir().join(format!(
            "deskwork-plugin-clone-{}-{}",
            chrono::Utc::now().timestamp_millis(),
            std::process::id()
        ));

        let install_result: Result<Vec<String>, String> = (|| {
            let clone_status = std::process::Command::new("git")
                .arg("clone")
                .arg("--depth")
                .arg("1")
                .arg(url)
                .arg(&temp_clone_dir)
                .status()
                .map_err(|err| format!("Failed to run git clone: {err}"))?;

            if !clone_status.success() {
                return Err(format!(
                    "git clone failed for `{url}` (exit code: {:?})",
                    clone_status.code()
                ));
            }

            let plugin_dirs = fs::read_dir(&temp_clone_dir)
                .map_err(|err| format!("Failed to inspect cloned repository: {err}"))?
                .filter_map(Result::ok)
                .filter_map(|entry| {
                    let is_dir = entry.file_type().ok()?.is_dir();
                    if !is_dir {
                        return None;
                    }

                    let plugin_manifest = entry.path().join(".claude-plugin").join("plugin.json");
                    if plugin_manifest.is_file() {
                        Some(entry.path())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            if plugin_dirs.is_empty() {
                return Err(
                    "No plugin folders found. Expected child directories containing `.claude-plugin/plugin.json`".to_string(),
                );
            }

            let plugins_dir = self.plugin_directory();
            fs::create_dir_all(&plugins_dir)
                .map_err(|err| format!("Failed to create plugins directory: {err}"))?;

            let mut installed = Vec::new();
            let mut failures = Vec::new();

            for plugin_dir in plugin_dirs {
                let Some(folder_name) = plugin_dir.file_name().and_then(|name| name.to_str())
                else {
                    failures.push(format!(
                        "Skipped plugin folder with invalid UTF-8 name: {}",
                        plugin_dir.display()
                    ));
                    continue;
                };

                let target_dir = plugins_dir.join(folder_name);

                if target_dir.exists() {
                    if let Err(err) = fs::remove_dir_all(&target_dir) {
                        failures.push(format!(
                            "Failed replacing existing plugin `{folder_name}`: {err}"
                        ));
                        continue;
                    }
                }

                if let Err(err) = copy_dir_all(&plugin_dir, &target_dir) {
                    failures.push(format!("Failed installing plugin `{folder_name}`: {err}"));
                    continue;
                }

                installed.push(folder_name.to_string());
            }

            if installed.is_empty() {
                if failures.is_empty() {
                    Err("No plugin folders were installed".to_string())
                } else {
                    Err(failures.join(" | "))
                }
            } else {
                if !failures.is_empty() {
                    warn!(
                        "Some plugin folders failed to install from git URL: {}",
                        failures.join(" | ")
                    );
                }
                Ok(installed)
            }
        })();

        if let Err(err) = fs::remove_dir_all(&temp_clone_dir) {
            warn!(
                "Failed to clean up temporary plugin clone directory {}: {}",
                temp_clone_dir.display(),
                err
            );
        }

        match install_result {
            Ok(installed_ids) => {
                self.reload_plugins();
                self.set_status(&format!(
                    "Installed plugin(s) from git: {}",
                    installed_ids.join(", ")
                ));
            }
            Err(err) => {
                self.set_status(&format!("Failed to install plugin(s) from git: {err}"));
            }
        }
    }

    /// Install (or replace) a plugin by copying a local folder into plugins dir.
    pub fn install_plugin_from_folder(&mut self, source_folder: &Path) {
        if !source_folder.is_dir() {
            self.set_status("Selected plugin source is not a folder");
            return;
        }

        let Some(folder_name) = source_folder.file_name() else {
            self.set_status("Selected plugin folder has no valid name");
            return;
        };

        let plugins_dir = self.plugin_directory();
        if let Err(err) = fs::create_dir_all(&plugins_dir) {
            self.set_status(&format!("Failed to create plugins directory: {err}"));
            return;
        }

        let target_dir = plugins_dir.join(folder_name);
        if target_dir.exists() {
            if let Err(err) = fs::remove_dir_all(&target_dir) {
                self.set_status(&format!(
                    "Failed to replace existing plugin `{}`: {err}",
                    target_dir.display()
                ));
                return;
            }
        }

        if let Err(err) = copy_dir_all(source_folder, &target_dir) {
            self.set_status(&format!("Failed to install plugin folder: {err}"));
            return;
        }

        self.reload_plugins();
        self.set_status(&format!(
            "Installed plugin folder `{}`",
            folder_name.to_string_lossy()
        ));
    }

    /// Enable or disable a plugin and persist the enabled set.
    pub fn set_plugin_enabled(&mut self, plugin_id: &str, enabled: bool) {
        self.plugin_runtime.toggle_plugin(plugin_id, enabled);

        if enabled {
            if !self
                .settings
                .plugins_enabled
                .iter()
                .any(|id| id == plugin_id)
            {
                self.settings.plugins_enabled.push(plugin_id.to_string());
            }
        } else {
            self.settings.plugins_enabled.retain(|id| id != plugin_id);
        }

        self.save_settings();
    }

    /// Get the effective plugin directory path.
    pub fn plugin_directory(&self) -> PathBuf {
        resolve_plugins_dir(&self.settings)
    }

    /// Check whether a bundled legal plugin is currently available.
    pub fn has_bundled_legal_plugin(&self) -> bool {
        self.plugin_runtime
            .registry()
            .all_plugins()
            .iter()
            .any(|p| p.id == "legal" && p.path.to_string_lossy().starts_with("bundled/"))
    }

    /// Clear the chat history.
    pub fn clear_chat(&mut self) {
        self.messages.clear();
        self.current_blocks.clear();
        self.streaming_block_kind = StreamingBlockKind::None;
    }

    /// Open a folder selection dialog asynchronously.
    pub fn open_folder_dialog(&mut self) {
        // Don't open another dialog if one is pending
        if self.folder_result_rx.is_some() {
            return;
        }

        let (tx, rx) = tokio::sync::oneshot::channel();
        self.folder_result_rx = Some(rx);

        self.runtime.spawn(async move {
            let folder = rfd::AsyncFileDialog::new()
                .pick_folder()
                .await
                .map(|f| f.path().to_path_buf());
            let _ = tx.send(folder);
        });
    }

    /// Check for folder selection completion.
    fn check_folder_selection(&mut self) {
        if let Some(mut rx) = self.folder_result_rx.take() {
            match rx.try_recv() {
                Ok(Some(folder)) => {
                    info!("Opened folder: {}", folder.display());
                    self.working_dir = Some(folder.clone());
                    self.settings.working_directory = Some(folder.to_string_lossy().to_string());
                    self.save_settings();
                    self.set_status(&format!("Opened: {}", folder.display()));
                }
                Ok(None) => {
                    // User cancelled the dialog
                    debug!("Folder selection cancelled");
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                    // Still waiting
                    self.folder_result_rx = Some(rx);
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    warn!("Folder selection channel closed unexpectedly");
                }
            }
        }
    }

    /// Refresh external tool statuses asynchronously.
    pub fn refresh_tool_statuses(&mut self) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.tool_status_rx = Some(rx);

        self.runtime.spawn(async move {
            let manager = match deskwork_core::ExternalToolManager::new() {
                Ok(m) => m,
                Err(e) => {
                    error!("Failed to create tool manager: {}", e);
                    let _ = tx.send(Vec::new());
                    return;
                }
            };

            let tools = manager.list_tools().await;
            let statuses: Vec<_> = tools
                .into_iter()
                .map(|t| {
                    let status_ui = ToolStatusUi {
                        is_installed: matches!(
                            t.status,
                            deskwork_core::ToolStatus::Installed { .. }
                        ),
                        version: if let deskwork_core::ToolStatus::Installed { version } = &t.status
                        {
                            Some(version.clone())
                        } else {
                            None
                        },
                        is_installing: false,
                        install_progress: 0,
                        is_supported: !matches!(
                            t.status,
                            deskwork_core::ToolStatus::UnsupportedPlatform
                        ),
                    };
                    (t.definition.id, status_ui)
                })
                .collect();

            let _ = tx.send(statuses);
        });
    }

    /// True if a tool status refresh is currently in-flight.
    pub fn is_refreshing_tool_statuses(&self) -> bool {
        self.tool_status_rx.is_some()
    }

    /// Check for tool status refresh completion.
    fn check_tool_status_completion(&mut self) {
        let Some(mut rx) = self.tool_status_rx.take() else {
            return;
        };

        match rx.try_recv() {
            Ok(statuses) => {
                for (id, mut status) in statuses {
                    // Preserve in-progress UI state so a refresh doesn't wipe the progress bar.
                    if let Some(existing) = self.tool_statuses.get(&id) {
                        if existing.is_installing {
                            status.is_installing = true;
                            status.install_progress = existing.install_progress;
                        }
                    }
                    self.tool_statuses.insert(id, status);
                }
            }
            Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                self.tool_status_rx = Some(rx);
            }
            Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                warn!("Tool status channel closed unexpectedly");
            }
        }
    }

    /// Start installing an external tool.
    pub fn start_tool_install(&mut self, tool_id: deskwork_core::ExternalToolId) {
        // Mark as installing in UI
        match self.tool_statuses.get_mut(&tool_id) {
            Some(status) => {
                if status.is_installing {
                    return;
                }
                status.is_installing = true;
                status.install_progress = 0;
            }
            None => {
                self.tool_statuses.insert(
                    tool_id,
                    ToolStatusUi {
                        is_installed: false,
                        version: None,
                        is_installing: true,
                        install_progress: 0,
                        is_supported: true,
                    },
                );
            }
        }

        let (progress_tx, progress_rx) = tokio::sync::mpsc::channel::<u8>(32);
        let (result_tx, result_rx) = tokio::sync::oneshot::channel();

        self.tool_install_progress_rx.push((tool_id, progress_rx));
        self.tool_install_result_rx.push((tool_id, result_rx));

        self.runtime.spawn(async move {
            let manager = match deskwork_core::ExternalToolManager::new() {
                Ok(m) => m,
                Err(e) => {
                    let _ = result_tx.send(Err(e.to_string()));
                    return;
                }
            };

            let ptx = progress_tx;
            let result = manager
                .install(tool_id, move |progress| {
                    if let Some(pct) = progress.percent {
                        let pct = pct.clamp(0.0, 100.0).round() as u8;
                        let _ = ptx.try_send(pct);
                    }
                })
                .await;

            let _ = result_tx.send(result.map_err(|e| e.to_string()));
        });
    }

    /// Start uninstalling an external tool.
    pub fn start_tool_uninstall(&mut self, tool_id: deskwork_core::ExternalToolId) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.tool_uninstall_result_rx.push((tool_id, rx));

        self.runtime.spawn(async move {
            let manager = match deskwork_core::ExternalToolManager::new() {
                Ok(m) => m,
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                    return;
                }
            };

            let result = manager.uninstall(tool_id).await;
            let _ = tx.send(result.map_err(|e| e.to_string()));
        });
    }

    /// Poll tool installation progress and results.
    fn check_tool_installs(&mut self) {
        // Check progress channels.
        for (tool_id, rx) in &mut self.tool_install_progress_rx {
            let mut last_progress = None;
            loop {
                match rx.try_recv() {
                    Ok(pct) => last_progress = Some(pct),
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
                }
            }

            if let Some(pct) = last_progress {
                if let Some(status) = self.tool_statuses.get_mut(tool_id) {
                    status.install_progress = pct;
                }
            }
        }

        // Check install result channels.
        let mut i = 0;
        while i < self.tool_install_result_rx.len() {
            let tool_id = self.tool_install_result_rx[i].0;
            let try_recv = {
                let (_, rx) = &mut self.tool_install_result_rx[i];
                rx.try_recv()
            };

            match try_recv {
                Ok(result) => {
                    // Remove result entry (swap_remove avoids shifting the whole vec).
                    let _ = self.tool_install_result_rx.swap_remove(i);

                    // Remove corresponding progress receiver (if present).
                    if let Some(pos) = self
                        .tool_install_progress_rx
                        .iter()
                        .position(|(id, _)| *id == tool_id)
                    {
                        let _ = self.tool_install_progress_rx.swap_remove(pos);
                    }

                    if let Some(status) = self.tool_statuses.get_mut(&tool_id) {
                        status.is_installing = false;
                        match result {
                            Ok(()) => {
                                status.is_installed = true;
                                status.install_progress = 100;
                                self.set_status(&format!("{} installed successfully", tool_id));
                            }
                            Err(e) => {
                                self.set_status(&format!("Failed to install {}: {}", tool_id, e));
                            }
                        }
                    }

                    // Refresh to get version info.
                    self.refresh_tool_statuses();
                    continue;
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                    i += 1;
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    let _ = self.tool_install_result_rx.swap_remove(i);

                    if let Some(pos) = self
                        .tool_install_progress_rx
                        .iter()
                        .position(|(id, _)| *id == tool_id)
                    {
                        let _ = self.tool_install_progress_rx.swap_remove(pos);
                    }

                    if let Some(status) = self.tool_statuses.get_mut(&tool_id) {
                        status.is_installing = false;
                    }
                    continue;
                }
            }
        }

        // Check uninstall result channels.
        let mut i = 0;
        while i < self.tool_uninstall_result_rx.len() {
            let tool_id = self.tool_uninstall_result_rx[i].0;
            let try_recv = {
                let (_, rx) = &mut self.tool_uninstall_result_rx[i];
                rx.try_recv()
            };

            match try_recv {
                Ok(result) => {
                    let _ = self.tool_uninstall_result_rx.swap_remove(i);
                    match result {
                        Ok(()) => {
                            if let Some(status) = self.tool_statuses.get_mut(&tool_id) {
                                status.is_installed = false;
                                status.version = None;
                            }
                            self.set_status(&format!("{} uninstalled", tool_id));
                        }
                        Err(e) => {
                            self.set_status(&format!("Failed to uninstall {}: {}", tool_id, e));
                        }
                    }
                    // Optional: refresh to keep supported/version fields correct.
                    self.refresh_tool_statuses();
                    continue;
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {
                    i += 1;
                }
                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                    let _ = self.tool_uninstall_result_rx.swap_remove(i);
                    continue;
                }
            }
        }
    }

    /// Handle files dropped onto the window.
    pub fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped_files: Vec<_> = ctx.input(|i| i.raw.dropped_files.clone());

        if dropped_files.is_empty() {
            return;
        }

        for file in dropped_files {
            // Try to get the file path
            if let Some(path) = &file.path {
                if attachments::is_image_file(path) {
                    if self.pending_attachments.len() >= attachments::MAX_ATTACHMENTS {
                        self.set_status(&format!(
                            "Maximum {} attachments reached",
                            attachments::MAX_ATTACHMENTS
                        ));
                        break;
                    }

                    match attachments::process_image_from_path(path, ctx) {
                        Ok(img) => {
                            info!("Added attachment: {}", img.filename);
                            self.pending_attachments.push(img);
                        }
                        Err(e) => {
                            warn!("Failed to process image {:?}: {}", path, e);
                            self.set_status(&format!("Failed to load image: {}", e));
                        }
                    }
                } else if attachments::is_text_file(path) {
                    match attachments::read_text_file_for_prompt(path, 20_000) {
                        Ok(content) => {
                            let filename = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("text-file");

                            if !self.input.is_empty() {
                                self.input.push_str("\n\n");
                            }

                            self.input.push_str(&format!(
                                "[Attached text file: {filename}]\n```text\n{content}\n```"
                            ));
                            self.set_status(&format!("Attached text file to prompt: {}", filename));
                        }
                        Err(e) => {
                            warn!("Failed to read dropped text file {:?}: {}", path, e);
                            self.set_status(&format!("Failed to read text file: {}", e));
                        }
                    }
                } else {
                    let ext = path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.to_ascii_lowercase());

                    match ext.as_deref() {
                        Some("pdf") | Some("doc") | Some("docx") => {
                            self.set_status(
                                "Document parsing for pdf/doc/docx is not available yet. Paste extracted text, or provide URL/path for tools.",
                            );
                        }
                        _ => self.set_status("Unsupported file type"),
                    }
                }
            } else if let Some(ref bytes) = file.bytes {
                // Dropped from another app (bytes only, no path)
                if self.pending_attachments.len() >= attachments::MAX_ATTACHMENTS {
                    self.set_status(&format!(
                        "Maximum {} attachments reached",
                        attachments::MAX_ATTACHMENTS
                    ));
                    break;
                }

                let filename = if file.name.is_empty() {
                    None
                } else {
                    Some(file.name.clone())
                };
                match attachments::process_image_from_bytes(bytes, filename, ctx) {
                    Ok(img) => {
                        info!("Added attachment from bytes: {}", img.filename);
                        self.pending_attachments.push(img);
                    }
                    Err(e) => {
                        warn!("Failed to process dropped bytes: {}", e);
                        self.set_status(&format!("Failed to load image: {}", e));
                    }
                }
            }
        }
    }
}

impl eframe::App for DeskworkApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for async completions
        self.check_auth_completion();
        self.check_models_completion();
        self.check_folder_selection();
        self.check_tool_status_completion();
        self.check_tool_installs();

        // Handle dropped files
        self.handle_dropped_files(ctx);

        // Auto-fetch models if authenticated but none loaded yet
        if matches!(self.auth_state, AuthState::Authenticated)
            && self.available_models.is_empty()
            && !self.fetching_models
        {
            debug!("Auto-fetching models on startup");
            self.fetch_models();
        }

        // Process any pending events from the agent
        self.process_events(ctx);
        self.clear_old_status();

        // Top panel with menu
        egui::TopBottomPanel::top("menu_panel").show(ctx, |ui| {
            ui::menu::render(self, ui, ctx);
        });

        // Status bar at bottom
        egui::TopBottomPanel::bottom("status_panel")
            .max_height(24.0)
            .show(ctx, |ui| {
                ui::status::render(self, ui);
            });

        // Input area above status bar (always visible at bottom)
        egui::TopBottomPanel::bottom("input_panel")
            .resizable(false)
            .show(ctx, |ui| {
                ui::input::render(self, ui);
            });

        // Settings modal if open
        if self.show_settings {
            ui::settings::render(self, ctx);
        }

        // Main chat area (fills remaining space)
        egui::CentralPanel::default().show(ctx, |ui| {
            ui::chat::render(self, ui);
        });

        // Request repaint if generating or waiting for async ops
        if self.is_generating
            || self.auth_result_rx.is_some()
            || self.models_result_rx.is_some()
            || self.folder_result_rx.is_some()
            || self.tool_status_rx.is_some()
            || !self.tool_install_progress_rx.is_empty()
            || !self.tool_install_result_rx.is_empty()
            || !self.tool_uninstall_result_rx.is_empty()
        {
            ctx.request_repaint();
        }
    }
}
