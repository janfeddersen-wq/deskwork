//! Settings dialog.

use eframe::egui::{self, RichText, Rounding, Vec2};

use crate::app::{AuthState, DeskworkApp};
use crate::ui::colors;

/// Render the settings dialog.
pub fn render(app: &mut DeskworkApp, ctx: &egui::Context) {
    egui::Window::new("Settings")
        .collapsible(false)
        .resizable(false)
        .default_width(450.0)
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            let muted = colors::muted(ui.visuals());

            ui.spacing_mut().item_spacing = Vec2::new(8.0, 12.0);

            // Authentication section
            ui.heading("Authentication");
            ui.separator();

            match &app.auth_state {
                AuthState::NotAuthenticated => {
                    ui.horizontal(|ui| {
                        ui.label("Status:");
                        ui.label(
                            RichText::new("Not signed in")
                                .color(colors::ERROR)
                                .size(14.0),
                        );
                    });

                    ui.add_space(8.0);

                    if ui
                        .add_sized(
                            Vec2::new(200.0, 36.0),
                            egui::Button::new(
                                RichText::new("Sign in with Claude")
                                    .size(14.0)
                                    .strong(),
                            )
                            .fill(colors::USER_BG)
                            .rounding(Rounding::same(8.0)),
                        )
                        .clicked()
                    {
                        app.start_auth();
                    }

                    ui.label(
                        RichText::new("Opens browser for OAuth authentication")
                            .size(11.0)
                            .color(muted)
                            .italics(),
                    );
                }

                AuthState::Authenticating => {
                    ui.horizontal(|ui| {
                        ui.label("Status:");
                        ui.spinner();
                        ui.label(
                            RichText::new("Waiting for browser authentication...")
                                .color(colors::USER_BG)
                                .size(14.0),
                        );
                    });

                    ui.label(
                        RichText::new("Complete the sign-in in your browser")
                            .size(11.0)
                            .color(muted)
                            .italics(),
                    );
                }

                AuthState::Authenticated => {
                    ui.horizontal(|ui| {
                        ui.label("Status:");
                        ui.label(
                            RichText::new("Signed in")
                                .color(colors::SUCCESS)
                                .size(14.0),
                        );
                    });

                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::Button::new("Refresh Models")
                                    .rounding(Rounding::same(8.0)),
                            )
                            .clicked()
                        {
                            app.fetch_models();
                        }

                        if app.fetching_models {
                            ui.spinner();
                        }

                        if ui
                            .add(
                                egui::Button::new(
                                    RichText::new("Sign Out").color(colors::ERROR),
                                )
                                .rounding(Rounding::same(8.0)),
                            )
                            .clicked()
                        {
                            app.sign_out();
                        }
                    });
                }

                AuthState::Error(msg) => {
                    ui.horizontal(|ui| {
                        ui.label("Status:");
                        ui.label(
                            RichText::new(format!("Error: {}", msg))
                                .color(colors::ERROR)
                                .size(14.0),
                        );
                    });

                    ui.add_space(8.0);

                    if ui
                        .add_sized(
                            Vec2::new(200.0, 36.0),
                            egui::Button::new(
                                RichText::new("Try Again")
                                    .size(14.0)
                                    .strong(),
                            )
                            .fill(colors::USER_BG)
                            .rounding(Rounding::same(8.0)),
                        )
                        .clicked()
                    {
                        app.start_auth();
                    }
                }
            }

            ui.add_space(16.0);

            // Model section
            ui.heading("Model");
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Claude Model:");
                ui.add_space(8.0);

                let model_label = if app.available_models.is_empty() {
                    app.settings.model_display_name()
                } else {
                    deskwork_core::model_display_name(&app.settings.model)
                };

                // Clone the models to avoid borrow issues
                let models: Vec<String> = app.available_models.clone();
                let mut selected_model: Option<String> = None;

                egui::ComboBox::from_id_salt("model_select")
                    .selected_text(model_label)
                    .show_ui(ui, |ui| {
                        if models.is_empty() {
                            ui.label(
                                RichText::new("Sign in to load models")
                                    .color(muted)
                                    .italics(),
                            );
                        } else {
                            for model in &models {
                                let is_selected = &app.settings.model == model;
                                let display = deskwork_core::model_display_name(model);
                                if ui.selectable_label(is_selected, &display).clicked() {
                                    selected_model = Some(model.clone());
                                }
                            }
                        }
                    });

                // Apply selection after ComboBox closes (avoids borrow conflict)
                if let Some(model) = selected_model {
                    app.settings.model = model;
                    app.save_settings();
                }
            });

            // Model description
            let model_desc = if app.settings.model.contains("sonnet") {
                "Best balance of speed and capability"
            } else if app.settings.model.contains("opus") {
                "Most capable, best for complex tasks"
            } else if app.settings.model.contains("haiku") {
                "Fastest, best for simple tasks"
            } else {
                ""
            };
            if !model_desc.is_empty() {
                ui.label(
                    RichText::new(model_desc)
                        .size(11.0)
                        .color(muted)
                        .italics(),
                );
            }

            ui.add_space(16.0);

            // Thinking settings
            ui.heading("Thinking");
            ui.separator();

            // Show thinking toggle
            ui.horizontal(|ui| {
                ui.checkbox(&mut app.settings.show_thinking, "Show thinking in chat");
                ui.label(
                    RichText::new("(display Claude's reasoning process)")
                        .size(11.0)
                        .color(muted),
                );
            });

            // Thinking budget
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label("Thinking Budget:");
                ui.add(
                    egui::Slider::new(&mut app.settings.thinking_budget, 1000..=32000)
                        .step_by(1000.0)
                        .show_value(true),
                );
            });
            ui.label(
                RichText::new("Maximum tokens for Claude's internal reasoning")
                    .size(11.0)
                    .color(muted),
            );

            ui.add_space(16.0);

            // Theme
            ui.heading("Appearance");
            ui.separator();

            ui.horizontal(|ui| {
                ui.label("Theme:");
                ui.add_space(8.0);

                let mut is_dark = app.settings.theme == deskwork_core::Theme::Dark;
                if ui.selectable_label(is_dark, "Dark").clicked() {
                    is_dark = true;
                }
                if ui.selectable_label(!is_dark, "Light").clicked() {
                    is_dark = false;
                }

                let new_theme = if is_dark {
                    deskwork_core::Theme::Dark
                } else {
                    deskwork_core::Theme::Light
                };

                if new_theme != app.settings.theme {
                    app.settings.theme = new_theme;
                    let visuals = if is_dark {
                        egui::Visuals::dark()
                    } else {
                        egui::Visuals::light()
                    };
                    ctx.set_visuals(visuals);
                }
            });

            ui.add_space(24.0);
            ui.separator();

            // Action buttons
            ui.horizontal(|ui| {
                if ui
                    .add_sized(
                        Vec2::new(100.0, 30.0),
                        egui::Button::new(RichText::new("Save").strong())
                            .fill(colors::USER_BG)
                            .rounding(Rounding::same(8.0)),
                    )
                    .clicked()
                {
                    app.save_settings();
                    app.show_settings = false;
                }

                if ui
                    .add_sized(
                        Vec2::new(100.0, 30.0),
                        egui::Button::new("Cancel").rounding(Rounding::same(8.0)),
                    )
                    .clicked()
                {
                    // Reload settings to discard changes
                    app.settings = deskwork_core::Settings::load(&app.db);
                    app.show_settings = false;
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(format!("v{}", deskwork_core::VERSION))
                            .size(11.0)
                            .color(muted),
                    );
                });
            });
        });
}
