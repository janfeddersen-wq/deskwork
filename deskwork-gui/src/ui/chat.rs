//! Chat message rendering.

use eframe::egui::{self, Color32, RichText, Rounding};

use crate::app::{ContentBlock, DeskworkApp, Message, MessageRole, ToolCall};
use crate::ui::colors;
use crate::ui::markdown::{self, MarkdownRenderState};

/// Render the main chat area.
pub fn render(app: &mut DeskworkApp, ui: &mut egui::Ui) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .stick_to_bottom(true)
        .show(ui, |ui| {
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(16.0, 8.0))
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());

                    if app.messages.is_empty() && !app.is_generating {
                        render_welcome(ui);
                    } else {
                        render_messages(app, ui);
                    }

                    if app.scroll_to_bottom {
                        ui.scroll_to_cursor(Some(egui::Align::BOTTOM));
                        app.scroll_to_bottom = false;
                    }
                });
        });
}

fn render_welcome(ui: &mut egui::Ui) {
    let muted = colors::muted(ui.visuals());

    ui.vertical_centered(|ui| {
        ui.add_space(80.0);
        ui.add_space(16.0);

        ui.label(RichText::new("Welcome to Deskwork").size(24.0).strong());
        ui.add_space(8.0);

        ui.label(
            RichText::new("Your Claude-powered coding assistant")
                .size(16.0)
                .color(muted),
        );
        ui.add_space(32.0);

        ui.label(
            RichText::new("Start by typing a message below, or try one of these:").color(muted),
        );
        ui.add_space(16.0);

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
                }
            }
        });
    });
}

fn render_messages(app: &mut DeskworkApp, ui: &mut egui::Ui) {
    let max_width = ui.available_width() * 0.8;

    let active_categories = app
        .category_registry
        .enabled_categories()
        .into_iter()
        .map(|c| c.name.clone())
        .collect::<Vec<_>>();

    if !active_categories.is_empty() {
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!("Active: {}", active_categories.join(", ")))
                .size(11.0)
                .color(colors::muted(ui.visuals())),
        );
    }

    for (msg_idx, message) in app.messages.iter().enumerate() {
        ui.add_space(8.0);
        render_message(app, ui, message, max_width, msg_idx);
    }

    if app.is_generating && !app.current_blocks.is_empty() {
        ui.add_space(8.0);
        render_streaming_response(app, ui, max_width);
    }
}

fn render_message(
    app: &DeskworkApp,
    ui: &mut egui::Ui,
    message: &Message,
    max_width: f32,
    msg_idx: usize,
) {
    match message.role {
        MessageRole::User => render_user_message(ui, message, max_width),
        MessageRole::Assistant => render_assistant_message(
            ui,
            message,
            max_width,
            msg_idx,
            app.settings.stream_markdown_enabled,
        ),
    }
}

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

fn render_assistant_message(
    ui: &mut egui::Ui,
    message: &Message,
    max_width: f32,
    msg_idx: usize,
    stream_markdown_enabled: bool,
) {
    let assistant_bg = colors::assistant_bg(ui.visuals());

    ui.vertical(|ui| {
        ui.set_max_width(max_width);

        for (block_idx, block) in message.blocks.iter().enumerate() {
            match block {
                ContentBlock::Thinking(thinking) => {
                    render_thinking_block(
                        ui,
                        thinking,
                        false,
                        format!("thinking-{}-{}", msg_idx, block_idx),
                    );
                }
                ContentBlock::ToolUse(tool_call) => {
                    render_tool_call(ui, tool_call, format!("tool-{}-{}", msg_idx, block_idx));
                }
                ContentBlock::Text(text) => {
                    if !text.is_empty() {
                        egui::Frame::none()
                            .fill(assistant_bg)
                            .rounding(Rounding::same(12.0))
                            .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                            .show(ui, |ui| {
                                ui.set_max_width(max_width);
                                render_response_with_status_badges(
                                    ui,
                                    text,
                                    stream_markdown_enabled,
                                    egui::Id::new((
                                        "assistant-message-markdown",
                                        msg_idx,
                                        block_idx,
                                    )),
                                );
                            });
                    }
                }
            }

            if block_idx + 1 < message.blocks.len() {
                ui.add_space(8.0);
            }
        }
    });
}

fn render_streaming_response(app: &DeskworkApp, ui: &mut egui::Ui, max_width: f32) {
    let assistant_bg = colors::assistant_bg(ui.visuals());

    ui.vertical(|ui| {
        ui.set_max_width(max_width);

        for (block_idx, block) in app.current_blocks.iter().enumerate() {
            match block {
                ContentBlock::Thinking(thinking) => {
                    if !thinking.is_empty() {
                        let is_active = block_idx == app.current_blocks.len() - 1;
                        render_thinking_block(
                            ui,
                            thinking,
                            is_active,
                            format!("thinking-streaming-{}", block_idx),
                        );
                    }
                }
                ContentBlock::ToolUse(tool_call) => {
                    render_tool_call(ui, tool_call, format!("tool-streaming-{}", block_idx));
                }
                ContentBlock::Text(text) => {
                    if !text.is_empty() {
                        egui::Frame::none()
                            .fill(assistant_bg)
                            .rounding(Rounding::same(12.0))
                            .inner_margin(egui::Margin::symmetric(12.0, 8.0))
                            .show(ui, |ui| {
                                ui.set_max_width(max_width);
                                render_response_with_status_badges(
                                    ui,
                                    text,
                                    app.settings.stream_markdown_enabled,
                                    egui::Id::new(("assistant-streaming-markdown", block_idx)),
                                );
                            });
                    }
                }
            }

            if block_idx + 1 < app.current_blocks.len() {
                ui.add_space(8.0);
            }
        }
    });
}

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
                ui.label(RichText::new(thinking).size(12.0).color(muted).italics());
            });
    });
}

fn render_response_with_status_badges(
    ui: &mut egui::Ui,
    content: &str,
    stream_markdown_enabled: bool,
    cache_key: egui::Id,
) {
    let mut segment_index = 0usize;
    let mut markdown_buffer = String::new();

    for line in content.lines() {
        let parts = split_status_tokens(line);

        if parts.badges.is_empty() {
            if !markdown_buffer.is_empty() {
                markdown_buffer.push('\n');
            }
            markdown_buffer.push_str(line);
            continue;
        }

        flush_render_buffer(
            ui,
            &mut markdown_buffer,
            stream_markdown_enabled,
            cache_key.with(segment_index),
        );
        segment_index += 1;

        ui.horizontal_wrapped(|ui| {
            for badge in &parts.badges {
                let color = match badge.as_str() {
                    "GREEN" | "🟢" => Color32::from_rgb(38, 166, 91),
                    "YELLOW" | "🟡" => Color32::from_rgb(219, 177, 54),
                    "RED" | "🔴" => Color32::from_rgb(214, 73, 73),
                    _ => Color32::GRAY,
                };

                egui::Frame::none()
                    .fill(color)
                    .rounding(Rounding::same(6.0))
                    .inner_margin(egui::Margin::symmetric(6.0, 2.0))
                    .show(ui, |ui| {
                        ui.label(RichText::new(badge).size(11.0).color(Color32::WHITE));
                    });
                ui.add_space(4.0);
            }
        });

        if !parts.remaining.trim().is_empty() {
            render_content(
                ui,
                &parts.remaining,
                stream_markdown_enabled,
                cache_key.with(segment_index),
            );
            segment_index += 1;
        }
    }

    flush_render_buffer(
        ui,
        &mut markdown_buffer,
        stream_markdown_enabled,
        cache_key.with(segment_index),
    );
}

fn flush_render_buffer(
    ui: &mut egui::Ui,
    markdown_buffer: &mut String,
    stream_markdown_enabled: bool,
    cache_key: egui::Id,
) {
    if markdown_buffer.trim().is_empty() {
        markdown_buffer.clear();
        return;
    }

    render_content(ui, markdown_buffer, stream_markdown_enabled, cache_key);
    markdown_buffer.clear();
}

fn render_content(
    ui: &mut egui::Ui,
    content: &str,
    stream_markdown_enabled: bool,
    cache_key: egui::Id,
) {
    if stream_markdown_enabled {
        let mut state = ui.ctx().data_mut(|data| {
            data.get_temp::<MarkdownRenderState>(cache_key)
                .unwrap_or_default()
        });
        markdown::render_markdown(ui, &mut state, content);
        ui.ctx().data_mut(|data| data.insert_temp(cache_key, state));
        return;
    }

    ui.add(egui::Label::new(RichText::new(content).size(14.0)).wrap());
}

#[derive(Debug, PartialEq, Eq)]
struct StatusLineParts {
    badges: Vec<String>,
    remaining: String,
}

fn split_status_tokens(line: &str) -> StatusLineParts {
    let leading_ws = line.len() - line.trim_start_matches(char::is_whitespace).len();
    let mut pos = leading_ws;
    let mut badges = Vec::new();

    loop {
        let tail = &line[pos..];
        if tail.is_empty() {
            break;
        }

        let token_end = tail.find(char::is_whitespace).unwrap_or(tail.len());
        let token = &tail[..token_end];
        let normalized = normalize_badge_token(token);

        if !is_status_badge(normalized) {
            break;
        }

        badges.push(normalized.to_string());
        pos += token_end;

        if pos >= line.len() {
            break;
        }

        let ws_len = line[pos..].len() - line[pos..].trim_start_matches(char::is_whitespace).len();
        let next_start = pos + ws_len;

        if next_start >= line.len() {
            break;
        }

        let next_tail = &line[next_start..];
        let next_token_end = next_tail
            .find(char::is_whitespace)
            .unwrap_or(next_tail.len());
        let next_token = &next_tail[..next_token_end];

        if is_status_badge(normalize_badge_token(next_token)) {
            pos = next_start;
            continue;
        }

        break;
    }

    if badges.is_empty() {
        return StatusLineParts {
            badges,
            remaining: line.to_string(),
        };
    }

    StatusLineParts {
        badges,
        remaining: line[pos..].to_string(),
    }
}

fn normalize_badge_token(token: &str) -> &str {
    token.trim_matches(|c: char| matches!(c, ':' | ';' | ',' | '.' | '(' | ')' | '[' | ']'))
}

fn is_status_badge(token: &str) -> bool {
    matches!(token, "GREEN" | "YELLOW" | "RED" | "🟢" | "🟡" | "🔴")
}

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
                                ui.label(RichText::new(result).size(11.0).monospace().color(color));
                            });
                    }
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_status_tokens_extracts_leading_badges_and_text() {
        let parts = split_status_tokens("GREEN 🟡 all systems nominal");
        assert_eq!(parts.badges, vec!["GREEN".to_string(), "🟡".to_string()]);
        assert_eq!(parts.remaining, " all systems nominal");
    }

    #[test]
    fn split_status_tokens_handles_leading_punctuation() {
        let parts = split_status_tokens("(RED): deploy failed.");
        assert_eq!(parts.badges, vec!["RED".to_string()]);
        assert_eq!(parts.remaining, " deploy failed.");
    }

    #[test]
    fn split_status_tokens_preserves_remainder_whitespace() {
        let parts = split_status_tokens("RED    spaced");
        assert_eq!(parts.badges, vec!["RED".to_string()]);
        assert_eq!(parts.remaining, "    spaced");
    }

    #[test]
    fn split_status_tokens_ignores_non_leading_badges() {
        let parts = split_status_tokens("info RED failed");
        assert!(parts.badges.is_empty());
        assert_eq!(parts.remaining, "info RED failed");
    }

    #[test]
    fn split_status_tokens_avoids_code_false_positive() {
        let parts = split_status_tokens("let color = RED;");
        assert!(parts.badges.is_empty());
        assert_eq!(parts.remaining, "let color = RED;");
    }
}
