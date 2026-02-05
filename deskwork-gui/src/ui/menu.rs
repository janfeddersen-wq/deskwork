//! Top menu bar.

use eframe::egui::{self, RichText};

use crate::app::{AuthState, DeskworkApp};
use crate::ui::colors;

/// Render the top menu bar.
pub fn render(app: &mut DeskworkApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    egui::menu::bar(ui, |ui| {
        // App title/logo
        ui.label(RichText::new("Deskwork").strong().size(15.0));
        ui.separator();

        // File menu
        ui.menu_button("File", |ui| {
            if ui.button("Open Folder...").clicked() {
                app.open_folder_dialog();
                ui.close_menu();
            }

            ui.separator();

            if ui.button("Clear Chat").clicked() {
                app.clear_chat();
                ui.close_menu();
            }

            ui.separator();

            if ui.button("Quit").clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });

        // Edit menu
        ui.menu_button("Edit", |ui| {
            if ui.button("Settings...").clicked() {
                app.show_settings = true;
                ui.close_menu();
            }
        });

        // View menu
        ui.menu_button("View", |ui| {
            let dark_mode = app.settings.theme == deskwork_core::Theme::Dark;

            if ui
                .checkbox(&mut { dark_mode }, "Dark Mode")
                .changed()
            {
                app.settings.theme = if !dark_mode {
                    deskwork_core::Theme::Dark
                } else {
                    deskwork_core::Theme::Light
                };

                // Apply theme immediately
                let visuals = if app.settings.theme == deskwork_core::Theme::Dark {
                    egui::Visuals::dark()
                } else {
                    egui::Visuals::light()
                };
                ctx.set_visuals(visuals);

                app.save_settings();
            }
        });

        // Help menu
        ui.menu_button("Help", |ui| {
            if ui.button("Documentation").clicked() {
                // TODO: Open docs
                ui.close_menu();
            }

            ui.separator();

            if ui.button("About").clicked() {
                // TODO: Show about dialog
                ui.close_menu();
            }
        });

        // Right-aligned status
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Generation indicator
            if app.is_generating {
                let time = ui.input(|i| i.time);
                let spinner = match ((time * 4.0) as i32) % 4 {
                    0 => ".",
                    1 => "..",
                    2 => "...",
                    _ => "",
                };
                ui.label(
                    RichText::new(format!("{} Generating...", spinner))
                        .color(colors::USER_BG)
                        .size(12.0),
                );
            }

            // Auth indicator
            match &app.auth_state {
                AuthState::NotAuthenticated => {
                    if ui
                        .link(RichText::new("Sign in").color(colors::ERROR).size(12.0))
                        .clicked()
                    {
                        app.start_auth();
                    }
                }
                AuthState::Authenticating => {
                    ui.spinner();
                    ui.label(
                        RichText::new("Signing in...")
                            .color(colors::USER_BG)
                            .size(12.0),
                    );
                }
                AuthState::Authenticated => {
                    ui.label(
                        RichText::new("Signed in")
                            .color(colors::SUCCESS)
                            .size(12.0),
                    );
                }
                AuthState::Error(_) => {
                    ui.label(
                        RichText::new("Auth error")
                            .color(colors::ERROR)
                            .size(12.0),
                    );
                }
            }
        });
    });
}
