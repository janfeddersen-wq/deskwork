//! Command bar — visual, clickable slash command chips above the input area.
//!
//! Shows available commands from enabled skill categories as interactive
//! pills/chips, so users never need to memorize `/category:command` syntax.

use std::collections::BTreeMap;

use eframe::egui::{self, RichText, Rounding};

use crate::app::DeskworkApp;
use crate::ui::colors;

/// Map well-known category IDs to emoji icons for visual scanning.
fn category_icon(id: &str) -> &str {
    match id {
        "legal" => "⚖️",
        "finance" => "💰",
        "sales" => "📈",
        "marketing" => "📣",
        "data" => "📊",
        "customer-support" => "🎧",
        "enterprise-search" => "🔍",
        "product-management" => "📋",
        "productivity" => "⚡",
        "bio-research" => "🧬",
        "cowork-plugin-management" => "🔌",
        _ => "📦",
    }
}

fn prettify_id(id: &str) -> String {
    id.split('-')
        .map(|word| {
            let mut c = word.chars();
            match c.next() {
                Some(f) => format!("{}{}", f.to_uppercase(), c.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Render the command bar.
///
/// Returns `true` if a command was selected (so caller can manage focus).
pub fn render(app: &mut DeskworkApp, ui: &mut egui::Ui) -> bool {
    // Don't render if toggled off
    if !app.show_command_bar {
        return false;
    }

    let commands = app.category_registry.all_slash_commands();

    // Nothing to show if no enabled categories have commands
    if commands.is_empty() {
        return false;
    }

    // Group commands by category (borrow keys to avoid per-frame allocations).
    let mut grouped: BTreeMap<&str, Vec<&deskwork_core::skills::types::CommandFile>> =
        BTreeMap::new();
    for cmd in &commands {
        grouped.entry(cmd.plugin_id.as_str()).or_default().push(cmd);
    }

    // Sort commands within each category for stable rendering.
    for cmds in grouped.values_mut() {
        cmds.sort_by(|a, b| a.name.cmp(&b.name));
    }

    let muted = colors::muted(ui.visuals());
    let border = colors::border(ui.visuals());
    let mut selected_command: Option<String> = None;

    // Draw a subtle top border
    let rect = ui.available_rect_before_wrap();
    ui.painter().line_segment(
        [rect.left_top(), rect.right_top()],
        egui::Stroke::new(0.5, border),
    );

    egui::Frame::none()
        .inner_margin(egui::Margin {
            left: 12.0,
            right: 12.0,
            top: 6.0,
            bottom: 6.0,
        })
        .show(ui, |ui| {
            // Horizontal scroll area for all the chips
            egui::ScrollArea::horizontal()
                .id_salt("command_bar_scroll")
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;

                        let category_count = grouped.len();

                        for (idx, (category_id, cmds)) in grouped.iter().enumerate() {
                            // CRITICAL: isolate each category to avoid widget ID collisions.
                            ui.push_id(category_id, |ui| {
                                // Category label with icon
                                let icon = category_icon(category_id);
                                ui.label(
                                    RichText::new(format!("{} {}", icon, prettify_id(category_id)))
                                        .size(11.0)
                                        .strong()
                                        .color(muted),
                                );

                                // Command chips
                                for cmd in cmds.iter().copied() {
                                    // CRITICAL: isolate each command to avoid collisions across categories.
                                    ui.push_id(&cmd.slash_command, |ui| {
                                        let has_args = cmd.argument_hint.is_some();
                                        let label = if has_args {
                                            format!("{} …", cmd.name)
                                        } else {
                                            cmd.name.clone()
                                        };

                                        let chip = egui::Button::new(
                                            RichText::new(&label).size(11.0),
                                        )
                                        .fill(colors::tool_bg(ui.visuals()))
                                        .rounding(Rounding::same(12.0));

                                        let response = ui.add(chip);
                                        let clicked = response.clicked();

                                        // Lazy tooltip allocation: build only on hover.
                                        response.on_hover_ui_at_pointer(|ui| {
                                            ui.label(
                                                RichText::new(&cmd.slash_command)
                                                    .size(11.0)
                                                    .strong(),
                                            );
                                            if !cmd.description.is_empty() {
                                                ui.label(
                                                    RichText::new(&cmd.description).size(11.0),
                                                );
                                            }
                                        });

                                        if clicked {
                                            selected_command = Some(cmd.slash_command.clone());
                                        }
                                    });
                                }

                                // Subtle separator between categories
                                if idx + 1 < category_count {
                                    ui.add(egui::Separator::default().vertical());
                                }
                            });
                        }
                    });
                });
        });

    // Apply selection
    if let Some(cmd) = selected_command {
        app.input = format!("{} ", cmd);
        return true;
    }

    false
}
