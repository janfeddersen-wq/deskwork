//! Main application state and update loop.

use eframe::egui;
use tokio::runtime::Runtime;
use tracing::{debug, error, info, warn};

use deskwork_core::{
    build_system_prompt, event_channel, run_agent, ClaudeCodeAuth, Database,
    EventReceiver, ExecutorEvent, ImageData, ImageMediaType, Settings,
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
    pub content: String,
    pub thinking: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    #[allow(dead_code)]
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Message {
    pub fn user(content: String) -> Self {
        Self {
            role: MessageRole::User,
            content,
            thinking: None,
            tool_calls: Vec::new(),
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn assistant() -> Self {
        Self {
            role: MessageRole::Assistant,
            content: String::new(),
            thinking: None,
            tool_calls: Vec::new(),
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

/// Main application state.
pub struct DeskworkApp {
    /// Tokio runtime for async operations.
    pub runtime: Runtime,

    /// Database connection.
    pub db: Database,

    /// User settings.
    pub settings: Settings,

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
    /// Current response being streamed.
    pub current_response: String,

    /// Current thinking being streamed.
    pub current_thinking: String,

    /// Current tool calls.
    pub current_tool_calls: Vec<ToolCall>,

    /// Event receiver for streaming.
    pub event_rx: Option<EventReceiver>,

    /// Handle to the generation task.
    pub generation_handle: Option<tokio::task::JoinHandle<()>>,

    // -------------------------------------------------------------------------
    // UI State
    // -------------------------------------------------------------------------
    /// Show settings dialog.
    pub show_settings: bool,

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

    /// Pending image attachments.
    pub pending_attachments: Vec<PendingImage>,
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
                Database::open_at(std::path::PathBuf::from(":memory:"))
                    .expect("Failed to create in-memory database")
            }
        };

        // Load settings
        let settings = Settings::load(&db);
        debug!(?settings, "Loaded settings");

        // Restore working directory from settings
        let working_dir = settings
            .working_directory
            .as_ref()
            .map(|s| std::path::PathBuf::from(s));

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
            auth_state,
            available_models,
            fetching_models: false,
            messages: Vec::new(),
            input: String::new(),
            is_generating: false,
            current_response: String::new(),
            current_thinking: String::new(),
            current_tool_calls: Vec::new(),
            event_rx: None,
            generation_handle: None,
            show_settings: false,
            working_dir,
            scroll_to_bottom: false,
            status_message: None,
            auth_result_rx: None,
            models_result_rx: None,
            folder_result_rx: None,
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
        let input = self.input.trim().to_string();
        if input.is_empty() {
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

        info!("Sending message: {}...", &input[..input.len().min(50)]);

        // Add user message
        self.messages.push(Message::user(input.clone()));
        self.input.clear();

        // Reset streaming state
        self.current_response.clear();
        self.current_thinking.clear();
        self.current_tool_calls.clear();
        self.is_generating = true;
        self.scroll_to_bottom = true;

        // Create event channel
        let (tx, rx) = event_channel();
        self.event_rx = Some(rx);

        // Build system prompt
        let system_prompt = build_system_prompt(self.settings.extended_thinking, None);

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
        let handle = self.runtime.spawn(async move {
            let handle = run_agent(
                access_token,
                model_name,
                settings,
                system_prompt,
                input,
                images,
                vec![], // TODO: Include message history
                tx,
            )
            .await;

            // Wait for completion
            let _ = handle.await;
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
                        self.current_response.push_str(&text);
                        ctx.request_repaint();
                    }

                    ExecutorEvent::ThinkingDelta(text) => {
                        self.current_thinking.push_str(&text);
                        ctx.request_repaint();
                    }

                    ExecutorEvent::ToolCallStart { id, name } => {
                        debug!(name, "Tool call started");
                        self.current_tool_calls.push(ToolCall::new(id, name));
                        ctx.request_repaint();
                    }

                    ExecutorEvent::ToolCallDelta { id: _, delta } => {
                        if let Some(tc) = self.current_tool_calls.last_mut() {
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
                        if let Some(tc) = self.current_tool_calls.last_mut() {
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
        if self.current_response.is_empty()
            && self.current_thinking.is_empty()
            && self.current_tool_calls.is_empty()
        {
            return;
        }

        let mut message = Message::assistant();
        message.content = std::mem::take(&mut self.current_response);

        if !self.current_thinking.is_empty() {
            message.thinking = Some(std::mem::take(&mut self.current_thinking));
        }

        message.tool_calls = std::mem::take(&mut self.current_tool_calls);
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

    /// Clear the chat history.
    pub fn clear_chat(&mut self) {
        self.messages.clear();
        self.current_response.clear();
        self.current_thinking.clear();
        self.current_tool_calls.clear();
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

    /// Handle files dropped onto the window.
    pub fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped_files: Vec<_> = ctx.input(|i| {
            i.raw.dropped_files.clone()
        });

        if dropped_files.is_empty() {
            return;
        }

        for file in dropped_files {
            // Check if we can add more attachments
            if self.pending_attachments.len() >= attachments::MAX_ATTACHMENTS {
                self.set_status(&format!(
                    "Maximum {} attachments reached",
                    attachments::MAX_ATTACHMENTS
                ));
                break;
            }

            // Try to get the file path
            if let Some(path) = &file.path {
                if attachments::is_image_file(path) {
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
                } else {
                    self.set_status("Only image files are supported");
                }
            } else if let Some(ref bytes) = file.bytes {
                // Dropped from another app (bytes only, no path)
                let filename = if file.name.is_empty() { None } else { Some(file.name.clone()) };
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

    /// Clear all pending attachments.
    pub fn clear_attachments(&mut self) {
        self.pending_attachments.clear();
    }
}

impl eframe::App for DeskworkApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Check for async completions
        self.check_auth_completion();
        self.check_models_completion();
        self.check_folder_selection();

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
        {
            ctx.request_repaint();
        }
    }
}
