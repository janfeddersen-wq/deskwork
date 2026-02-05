//! Chat message rendering.

use eframe::egui::{self, Color32, RichText, Rounding};

use crate::app::{DeskworkApp, Message, MessageRole, ToolCall};
use crate::ui::colors;

/// Render the main chat area.
pub fn render(app: &mut DeskworkApp, ui: &mut egui::Ui) {
    // Messages area with scroll - fills the entire central panel
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            // Apply margins to content inside scroll area
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(16.0, 8.0))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());

                    if app.messages.is_empty() && !app.is_generating {
                        render_welcome(ui);
                    } else {
                        render_messages(app, ui);
                    }

                    // Handle scroll to bottom
                    if app.scroll_to_bottom {
                        ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                        app.scroll_to_bottom = false;
                    }
                });
        });
}

/// Render welcome message when chat is empty.
fn render_welcome(ui: &mut egui::Ui) {
    let muted = colors::muted(ui.visuals());

    ui.vertical_centered(|ui| {
        ui.add_space(80.0);

        // Welcome header
        ui.add_space(16.0);

        ui.label(
            RichText::new("Welcome to Deskwork")
                .size(24.0)
                .strong(),
        );
        ui.add_space(8.0);

        ui.label(
            RichText::new("Your Claude-powered coding assistant")
                .size(16.0)
                .color(muted),
        );
        ui.add_space(32.0);

        ui.label(
            RichText::new("Start by typing a message below, or try one of these:")
                .color(muted),
        );
        ui.add_space(16.0);

        // Suggestion buttons
        ui.horizontal_wrapped(|ui| {
            ui.add_space(ui.available_width() / 4.0);

            let suggestions = [
                "Explore this directory",
                "Find TODOs in the codebase",
                "Help me write a function",
            ];

            for suggestion in suggestions {
                if ui
                    .add(
                        egui::Button::new(RichText::new(suggestion).size(13.0))
                            .rounding(Rounding::same(8.0)),
                    )
                    .clicked()
                {
                    let _text = suggestion.to_string();
                    // This would need to be handled differently
                    // For now, just a visual indicator
                }
            }
        });
    });
}

/// Render all messages.
fn render_messages(app: &mut DeskworkApp, ui: &mut egui::Ui) {
    let max_width = ui.available_width() * 0.8;

    for (msg_idx, message) in app.messages.iter().enumerate() {
        ui.add_space(8.0);
        render_message(ui, message, max_width, msg_idx);
    }

    // Render current streaming response
    if app.is_generating
        && (!app.current_response.is_empty()
            || !app.current_thinking.is_empty()
            || !app.current_tool_calls.is_empty())
    {
        ui.add_space(8.0);
        render_streaming_response(app, ui, max_width);
    }
}

/// Render a single message.
fn render_message(ui: &mut egui::Ui, message: &Message, max_width: f32, msg_idx: usize) {
    match message.role {
        MessageRole::User => render_user_message(ui, message, max_width),
        MessageRole::Assistant => render_assistant_message(ui, message, max_width, msg_idx),
    }
}

/// Render a user message (right-aligned, blue bubble).
fn render_user_message(ui: &mut egui::Ui, message: &Message, max_width: f32) {
    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
        egui::Frame::none()
            .fill(colors::USER_BG)
            .rounding(Rounding {
                nw: 16.0,
                ne: 4.0,
                sw: 16.0,
                se: 16.0,
            })
            .inner_margin(egui::Margin::symmetric(12.0, 8.0))
            .show(ui, |ui| {
                ui.set_max_width(max_width);
                ui.label(
                    RichText::new(&message.content)
                        .color(Color32::WHITE)
                        .size(14.0),
                );
            });
    });
}

/// Render an assistant message (left-aligned, with optional thinking and tools).
fn render_assistant_message(ui: &mut egui::Ui, message: &Message, max_width: f32, msg_idx: usize) {
    let assistant_bg = colors::assistant_bg(ui.visuals());

    ui.vertical(|ui| {
        ui.set_max_width(max_width);

        // Thinking block (collapsible)
        if let Some(thinking) = &message.thinking {
            render_thinking_block(ui, thinking, false, format!("thinking-{}", msg_idx));
            ui.add_space(8.0);
        }

        // Tool calls
        for (tool_idx, tool_call) in message.tool_calls.iter().enumerate() {
            render_tool_call(ui, tool_call, format!("tool-{}-{}", msg_idx, tool_idx));
            ui.add_space(8.0);
        }

        // Main response
        if !message.content.is_empty() {
            egui::Frame::none()
                .fill(assistant_bg)
                .rounding(Rounding::same(12.0))
                .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                .show(ui, |ui| {
                    ui.set_max_width(max_width);
                    ui.label(RichText::new(&message.content).size(14.0));
                });
        }
    });
}

/// Render the current streaming response.
fn render_streaming_response(app: &DeskworkApp, ui: &mut egui::Ui, max_width: f32) {
    let assistant_bg = colors::assistant_bg(ui.visuals());

    ui.vertical(|ui| {
        ui.set_max_width(max_width);

        // Thinking block
        if !app.current_thinking.is_empty() {
            render_thinking_block(ui, &app.current_thinking, true, "thinking-streaming".to_string());
            ui.add_space(8.0);
        }

        // Tool calls
        for (tool_idx, tool_call) in app.current_tool_calls.iter().enumerate() {
            render_tool_call(ui, tool_call, format!("tool-streaming-{}", tool_idx));
            ui.add_space(8.0);
        }

        // Current response text
        if !app.current_response.is_empty() {
            egui::Frame::none()
                .fill(assistant_bg)
                .rounding(Rounding::same(12.0))
                .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                .show(ui, |ui| {
                    ui.set_max_width(max_width);
                    ui.label(RichText::new(&app.current_response).size(14.0));
                });
        }
    });
}

/// Render a thinking block.
fn render_thinking_block(ui: &mut egui::Ui, thinking: &str, is_streaming: bool, id_source: String) {
    let muted = colors::muted(ui.visuals());
    let thinking_bg = colors::thinking_bg(ui.visuals());

    egui::CollapsingHeader::new(
        RichText::new(if is_streaming {
            "Thinking..."
        } else {
            "Thinking"
        })
        .size(12.0)
        .color(muted),
    )
    .id_salt(id_source)
    .default_open(is_streaming)
    .show(ui, |ui| {
        egui::Frame::none()
            .fill(thinking_bg)
            .rounding(Rounding::same(8.0))
            .inner_margin(egui::Margin::same(8.0))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(thinking)
                        .size(12.0)
                        .color(muted)
                        .italics(),
                );
            });
    });
}

/// Render a tool call.
fn render_tool_call(ui: &mut egui::Ui, tool_call: &ToolCall, id_source: String) {
    let muted = colors::muted(ui.visuals());
    let tool_bg = colors::tool_bg(ui.visuals());

    let header_text = if tool_call.result.is_some() {
        let icon = if tool_call.success { "[ok]" } else { "[err]" };
        let color = if tool_call.success {
            colors::SUCCESS
        } else {
            colors::ERROR
        };
        RichText::new(format!("{} {}", icon, tool_call.name))
            .size(12.0)
            .color(color)
    } else {
        RichText::new(format!("[...] {}", tool_call.name))
            .size(12.0)
            .color(muted)
    };

    let id_source_clone = id_source.clone();
    egui::CollapsingHeader::new(header_text)
        .id_salt(id_source)
        .default_open(false)
        .show(ui, |ui| {
            egui::Frame::none()
                .fill(tool_bg)
                .rounding(Rounding::same(8.0))
                .inner_margin(egui::Margin::same(8.0))
                .show(ui, |ui| {
                    // Arguments
                    if !tool_call.arguments.is_empty() {
                        ui.label(RichText::new("Arguments:").size(11.0).strong());
                        egui::ScrollArea::vertical()
                            .id_salt(format!("{}-args", id_source_clone))
                            .max_height(100.0)
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(&tool_call.arguments)
                                        .size(11.0)
                                        .monospace()
                                        .color(muted),
                                );
                            });
                    }

                    // Result
                    if let Some(result) = &tool_call.result {
                        ui.add_space(8.0);
                        ui.label(RichText::new("Result:").size(11.0).strong());
                        egui::ScrollArea::vertical()
                            .id_salt(format!("{}-result", id_source_clone))
                            .max_height(150.0)
                            .show(ui, |ui| {
                                let color = if tool_call.success {
                                    muted
                                } else {
                                    colors::ERROR
                                };
                                ui.label(
                                    RichText::new(result)
                                        .size(11.0)
                                        .monospace()
                                        .color(color),
                                );
                            });
                    }
                });
        });
}
