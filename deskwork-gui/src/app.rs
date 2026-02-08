//! Main application state and update loop.

use eframe::egui;
use tokio::runtime::Runtime;
use tracing::{debug, error, info, warn};

use std::collections::HashMap;
use std::path::PathBuf;

use deskwork_core::{
    build_system_prompt, event_channel, run_agent, ClaudeCodeAuth, Database, DocumentData,
    DocumentMediaType, EventReceiver, ExecutorEvent, ImageData, ImageMediaType, ModelRequest,
    RunAgentArgs, Settings,
};
use deskwork_core::skills::categories::{build_mcp_map, McpBridgeResult, SkillCategoryRegistry};
use deskwork_core::skills::category_context::{build_category_context, ContextBudget};
use deskwork_core::skills::commands::{self as skill_commands};

use crate::ui;
use crate::ui::attachments::{self, PendingDocument, PendingImage};

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

    /// Skill category registry (replaces plugin_runtime).
    pub category_registry: SkillCategoryRegistry,

    /// Cached MCP bridge result for skill categories.
    pub category_mcp: McpBridgeResult,

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
    /// Message history (UI display).
    pub messages: Vec<Message>,

    /// API-level message history for conversation continuity.
    /// Stored as-is from the agent framework to preserve exact types,
    /// thinking signatures, and tool call IDs.
    pub api_history: Vec<ModelRequest>,

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

    /// Whether to show the command bar above the input area.
    pub show_command_bar: bool,

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

    /// Pending document attachments (PDFs).
    pub pending_documents: Vec<PendingDocument>,
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

        // Initialize skill category registry
        let category_registry = SkillCategoryRegistry::load(&settings.plugins_enabled);
        let category_mcp = build_mcp_map(&category_registry.enabled_categories());

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

        // Configure custom fonts for comprehensive Unicode support.
        // egui's default fonts (Ubuntu-Light, Hack) have limited Unicode coverage,
        // causing rendering issues with characters LLMs commonly produce (em dashes,
        // curly quotes, arrows, math symbols, emoji, etc.).
        let mut fonts = egui::FontDefinitions::default();

        // -- Insert font data --
        fonts.font_data.insert(
            "Inter".to_owned(),
            egui::FontData::from_static(include_bytes!("../fonts/Inter-Regular.ttf")),
        );
        fonts.font_data.insert(
            "JetBrainsMono".to_owned(),
            egui::FontData::from_static(include_bytes!("../fonts/JetBrainsMono-Regular.ttf")),
        );
        fonts.font_data.insert(
            "NotoSansSymbols2".to_owned(),
            egui::FontData::from_static(include_bytes!("../fonts/NotoSansSymbols2-Regular.ttf")),
        );
        fonts.font_data.insert(
            "NotoEmoji".to_owned(),
            egui::FontData::from_static(include_bytes!("../fonts/NotoEmoji-Regular.ttf")),
        );

        // -- Configure proportional font family (body text, headings, UI) --
        // Insert our fonts at the front so they take priority over egui defaults.
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
            family.insert(0, "NotoEmoji".to_owned());
            family.insert(0, "NotoSansSymbols2".to_owned());
            family.insert(0, "Inter".to_owned());
        }

        // -- Configure monospace font family (code blocks, inline code) --
        if let Some(family) = fonts.families.get_mut(&egui::FontFamily::Monospace) {
            family.insert(0, "NotoEmoji".to_owned());
            family.insert(0, "NotoSansSymbols2".to_owned());
            family.insert(0, "JetBrainsMono".to_owned());
        }

        cc.egui_ctx.set_fonts(fonts);

        // Configure spacing for better readability
        let mut style = (*cc.egui_ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        cc.egui_ctx.set_style(style);

        Self {
            runtime,
            db,
            settings,
            category_registry,
            category_mcp,
            skills_context,
            auth_state,
            available_models,
            fetching_models: false,
            messages: Vec::new(),
            api_history: vec![],
            input: String::new(),
            is_generating: false,
            current_blocks: Vec::new(),
            streaming_block_kind: StreamingBlockKind::None,
            event_rx: None,
            generation_handle: None,
            show_settings: false,
            settings_tab: Default::default(),
            show_command_bar: true,
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
            pending_documents: Vec::new(),
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

        // Resolve slash commands into full command prompts.
        let mut agent_input = raw_input.clone();
        if raw_input.starts_with('/') {
            let user_inputs = parse_inline_command_inputs(&raw_input);
            if let Some(parsed) = skill_commands::parse_slash_command(&raw_input) {
                match skill_commands::get_command_handler(&self.category_registry, &parsed.slash_command)
                {
                    Some(command) => {
                        agent_input = skill_commands::build_command_prompt(
                            command,
                            &user_inputs,
                            parsed.raw_args.as_deref(),
                        );
                    }
                    None => {
                        self.set_status(&format!(
                            "No enabled command handler found for {}",
                            parsed.slash_command
                        ));
                        return;
                    }
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

        let budget = ContextBudget {
            max_tokens: self.settings.plugin_context_token_budget as usize,
        };
        let category_context =
            build_category_context(&self.category_registry, &self.category_mcp, budget);

        let skills_prompt = self
            .skills_context
            .as_ref()
            .map(|ctx| ctx.to_prompt_section(self.settings.working_directory.as_deref()));

        // Build system prompt
        let system_prompt = build_system_prompt(
            self.settings.extended_thinking,
            None,
            Some(category_context.prompt.as_str()),
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

        // Convert pending documents to DocumentData
        let documents: Vec<DocumentData> = self
            .pending_documents
            .iter()
            .map(|doc| DocumentData {
                data: doc.data.clone(),
                media_type: DocumentMediaType::Pdf,
                filename: Some(doc.filename.clone()),
            })
            .collect();

        // Spawn the agent
        let settings = self.settings.clone();
        let model_name = settings.model.clone();
        let plugin_mcp_configs = self.category_mcp.configs.clone();
        let api_history = self.api_history.clone();
        let handle = self.runtime.spawn(async move {
            let agent_handle = run_agent(RunAgentArgs {
                access_token,
                model_name,
                settings,
                system_prompt,
                user_input: agent_input,
                images,
                documents,
                message_history: api_history,
                plugin_mcp_configs,
                event_sender: tx,
            });

            // Wait for completion
            let _ = agent_handle.await;
        });

        // Clear attachments after sending
        self.pending_attachments.clear();
        self.pending_documents.clear();

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
                        message_history,
                    } => {
                        info!(input_tokens, output_tokens, "Generation complete");
                        self.api_history = message_history;
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

    /// Reload skill categories from bundled assets.
    pub fn reload_categories(&mut self) {
        self.category_registry = SkillCategoryRegistry::load(&self.settings.plugins_enabled);
        self.category_mcp = build_mcp_map(&self.category_registry.enabled_categories());
        self.set_status("Skill categories reloaded");
    }




    pub fn set_category_enabled(&mut self, category_id: &str, enabled: bool) {
        if enabled {
            self.category_registry.enable(category_id);
            if !self
                .settings
                .plugins_enabled
                .iter()
                .any(|id| id == category_id)
            {
                self.settings.plugins_enabled.push(category_id.to_string());
            }
        } else {
            self.category_registry.disable(category_id);
            self.settings.plugins_enabled.retain(|id| id != category_id);
        }

        // Rebuild MCP map after category change
        self.category_mcp = build_mcp_map(&self.category_registry.enabled_categories());
        self.save_settings();
    }



    /// Clear the chat history.
    pub fn clear_chat(&mut self) {
        self.messages.clear();
        self.api_history.clear();
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
                } else if attachments::is_pdf_file(path) {
                    if self.pending_documents.len() >= attachments::MAX_ATTACHMENTS {
                        self.set_status(&format!(
                            "Maximum {} attachments reached",
                            attachments::MAX_ATTACHMENTS
                        ));
                        break;
                    }

                    match attachments::process_pdf_from_path(path) {
                        Ok(doc) => {
                            info!("Added PDF attachment: {}", doc.filename);
                            self.pending_documents.push(doc);
                        }
                        Err(e) => {
                            warn!("Failed to process PDF {:?}: {}", path, e);
                            self.set_status(&format!("Failed to load PDF: {}", e));
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
                } else if attachments::is_office_file(path) {
                    // Office documents: copy to working directory and add prompt
                    if let Some(ref work_dir) = self.working_dir {
                        let filename = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "document".to_string());

                        let dest = {
                            let initial = work_dir.join(&filename);
                            if !initial.exists() {
                                initial
                            } else {
                                // Find a unique name: file (1).docx, file (2).docx, ...
                                let stem = std::path::Path::new(&filename)
                                    .file_stem()
                                    .and_then(|s| s.to_str())
                                    .unwrap_or(filename.as_str());
                                let ext = std::path::Path::new(&filename)
                                    .extension()
                                    .and_then(|e| e.to_str())
                                    .unwrap_or("");
                                let mut counter = 1u32;
                                loop {
                                    let candidate = if ext.is_empty() {
                                        work_dir.join(format!("{stem} ({counter})"))
                                    } else {
                                        work_dir.join(format!("{stem} ({counter}).{ext}"))
                                    };
                                    if !candidate.exists() {
                                        break candidate;
                                    }
                                    counter += 1;
                                    if counter > 100 {
                                        // Safety valve
                                        break candidate;
                                    }
                                }
                            }
                        };

                        match std::fs::copy(path, &dest) {
                            Ok(_) => {
                                let dest_filename = dest
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .unwrap_or(filename.as_str());
                                info!("Copied Office file to working dir: {}", dest.display());

                                if !self.input.is_empty() {
                                    self.input.push_str("\n\n");
                                }
                                let safe_name = dest_filename.replace(['\r', '\n'], " ");
                                self.input
                                    .push_str(&format!("Please look at `./{safe_name}`"));
                                self.set_status(&format!(
                                    "Copied {} to working directory",
                                    dest_filename
                                ));
                            }
                            Err(e) => {
                                warn!("Failed to copy Office file {:?}: {}", path, e);
                                self.set_status(&format!("Failed to copy file: {}", e));
                            }
                        }
                    } else {
                        self.set_status(
                            "Set a working directory first (File → Open Folder)",
                        );
                    }
                } else {
                    self.set_status("Unsupported file type");
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
