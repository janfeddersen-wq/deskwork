//! Input area for sending messages.

use std::collections::BTreeMap;

use eframe::egui::{self, Color32, Key, RichText, Rounding, Vec2};

use crate::app::DeskworkApp;
use crate::ui::attachments;
use crate::ui::colors;

/// Render the input area.
pub fn render(app: &mut DeskworkApp, ui: &mut egui::Ui) {
    let muted = colors::muted(ui.visuals());
    let code_bg = colors::code_bg(ui.visuals());
    let border = colors::border(ui.visuals());

    // Draw top border line
    let rect = ui.available_rect_before_wrap();
    ui.painter().line_segment(
        [rect.left_top(), rect.right_top()],
        egui::Stroke::new(1.0, border),
    );

    // Render attachment previews if any
    if !app.pending_attachments.is_empty() {
        egui::Frame::none()
            .inner_margin(egui::Margin {
                left: 12.0,
                right: 12.0,
                top: 8.0,
                bottom: 0.0,
            })
            .show(ui, |ui| {
                attachments::render_attachments(&mut app.pending_attachments, ui);
            });
    }

    egui::Frame::none()
        .fill(code_bg)
        .inner_margin(egui::Margin {
            left: 12.0,
            right: 12.0,
            top: 12.0,
            bottom: 8.0,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                // Text input - grows up to 4 lines, then scrolls
                let text_edit = egui::TextEdit::multiline(&mut app.input)
                    .desired_width(ui.available_width() - 100.0)
                    .desired_rows(2)
                    .lock_focus(true)
                    .hint_text("Ask Claude something... (Enter to send, Shift+Enter for newline)")
                    .font(egui::TextStyle::Body)
                    .frame(false);

                // Wrap in scroll area with max height of ~4 lines
                let line_height = ui.text_style_height(&egui::TextStyle::Body);
                let max_height = line_height * 4.5; // ~4 lines plus a bit of padding

                let response = egui::ScrollArea::vertical()
                    .max_height(max_height)
                    .show(ui, |ui| ui.add(text_edit))
                    .inner;

                // Handle Enter to send (without shift)
                if response.has_focus() {
                    let enter_pressed =
                        ui.input(|i| i.key_pressed(Key::Enter) && !i.modifiers.shift);

                    if enter_pressed && !app.input.trim().is_empty() && !app.is_generating {
                        // Remove the newline that was just added
                        if app.input.ends_with('\n') {
                            app.input.pop();
                        }
                        app.send_message();
                    }
                }

                ui.add_space(8.0);

                // Buttons
                ui.vertical(|ui| {
                    ui.add_space(4.0);

                    if app.is_generating {
                        // Stop button
                        if ui
                            .add_sized(
                                Vec2::new(70.0, 32.0),
                                egui::Button::new(RichText::new("Stop").color(colors::ERROR))
                                    .fill(Color32::from_rgb(60, 40, 40))
                                    .rounding(Rounding::same(8.0)),
                            )
                            .clicked()
                        {
                            app.stop_generation();
                        }
                    } else {
                        // Send button
                        let can_send = !app.input.trim().is_empty() && app.is_authenticated();

                        let button = egui::Button::new(RichText::new("Send").color(if can_send {
                            Color32::WHITE
                        } else {
                            muted
                        }))
                        .fill(if can_send {
                            colors::USER_BG
                        } else {
                            colors::tool_bg(ui.visuals())
                        })
                        .rounding(Rounding::same(8.0));

                        if ui
                            .add_sized(Vec2::new(70.0, 32.0), button)
                            .on_hover_text_at_pointer(if !app.is_authenticated() {
                                "Sign in first"
                            } else if app.input.trim().is_empty() {
                                "Type a message first"
                            } else {
                                "Send message"
                            })
                            .clicked()
                            && can_send
                        {
                            app.send_message();
                        }
                    }
                });
            });

            // Slash command suggestions
            if app.input.trim_start().starts_with('/') {
                let prefix = app.input.split_whitespace().next().unwrap_or_default();
                let suggestions = app.plugin_runtime.command_suggestions_rich(prefix);

                if !suggestions.is_empty() {
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new("Slash command suggestions")
                            .size(11.0)
                            .color(muted),
                    );

                    let mut grouped: BTreeMap<String, Vec<deskwork_core::SlashCommandSuggestion>> =
                        BTreeMap::new();
                    for suggestion in suggestions.into_iter().take(12) {
                        grouped
                            .entry(suggestion.plugin_id.clone())
                            .or_default()
                            .push(suggestion);
                    }

                    let mut selected: Option<String> = None;

                    for (plugin_id, items) in grouped {
                        ui.add_space(4.0);
                        ui.label(RichText::new(plugin_id).size(11.0).strong().color(muted));

                        for suggestion in items {
                            let short_description = if suggestion.description.trim().is_empty() {
                                "No description".to_string()
                            } else {
                                suggestion.description.chars().take(80).collect::<String>()
                            };

                            let label =
                                format!("{} — {}", suggestion.slash_command, short_description);

                            if ui
                                .add(
                                    egui::Button::new(RichText::new(label).size(11.0))
                                        .fill(colors::tool_bg(ui.visuals()))
                                        .rounding(Rounding::same(6.0)),
                                )
                                .clicked()
                            {
                                selected = Some(suggestion.slash_command.clone());
                            }
                        }
                    }

                    if let Some(suggestion) = selected {
                        app.input = format!("{suggestion} ");
                    }
                }
            }

            // Character count and hints
            ui.horizontal(|ui| {
                let char_count = app.input.len();
                ui.label(
                    RichText::new(format!("{} chars", char_count))
                        .size(10.0)
                        .color(muted),
                );

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !app.is_authenticated() {
                        if ui
                            .link(RichText::new("Sign in").size(11.0).color(colors::ERROR))
                            .clicked()
                        {
                            app.start_auth();
                        }
                    } else {
                        ui.label(
                            RichText::new(format!("Model: {}", app.settings.model_display_name()))
                                .size(10.0)
                                .color(muted),
                        );
                    }
                });
            });
        });
}
