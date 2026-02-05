//! Status bar at the bottom of the window.

use eframe::egui::{self, RichText};

use crate::app::DeskworkApp;
use crate::ui::colors;

/// Render the status bar.
pub fn render(app: &DeskworkApp, ui: &mut egui::Ui) {
    let muted = colors::muted(ui.visuals());

    ui.horizontal(|ui| {
        // Status message
        if let Some((msg, _)) = &app.status_message {
            ui.label(RichText::new(msg).size(11.0).color(muted));
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Message count
            let msg_count = app.messages.len();
            if msg_count > 0 {
                ui.label(
                    RichText::new(format!("{} messages", msg_count))
                        .size(11.0)
                        .color(muted),
                );
                ui.separator();
            }

            // Current working directory
            if let Some(ref dir) = app.working_dir {
                let path_str = dir.to_string_lossy();
                let display_path = if path_str.len() > 50 {
                    format!("...{}", &path_str[path_str.len() - 47..])
                } else {
                    path_str.to_string()
                };
                ui.label(
                    RichText::new(format!("📁 {}", display_path))
                        .size(11.0)
                        .color(muted),
                );
            }
        });
    });
}
