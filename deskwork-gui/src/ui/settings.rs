//! Settings dialog.

use eframe::egui::{self, RichText, Rounding, Vec2};

use crate::app::{AuthState, DeskworkApp};
use crate::ui::colors;
use deskwork_core::external_tools::get_all_tool_definitions;

/// Active tab in the settings dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsTab {
    #[default]
    General,
    Appearance,
    Skills, // was "Plugins" — now shows skill categories + python tools
    Tools,
}

/// Render the settings dialog.
pub fn render(app: &mut DeskworkApp, ctx: &egui::Context) {
    egui::Window::new("Settings")
        .collapsible(false)
        .resizable(false)
        .default_width(520.0)
        .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
        .show(ctx, |ui| {
            let muted = colors::muted(ui.visuals());

            ui.spacing_mut().item_spacing = Vec2::new(8.0, 12.0);

            // -----------------------------------------------------------------
            // Tabs
            // -----------------------------------------------------------------
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                for (tab, label) in [
                    (SettingsTab::General, "  General  "),
                    (SettingsTab::Appearance, "  Appearance  "),
                    (SettingsTab::Skills, "  Skills  "),
                    (SettingsTab::Tools, "  Tools  "),
                ] {
                    let selected = app.settings_tab == tab;
                    let response = ui.selectable_label(selected, RichText::new(label).size(14.0));
                    if response.clicked() {
                        app.settings_tab = tab;
                    }
                }
            });
            ui.separator();

            // -----------------------------------------------------------------
            // Tab content
            // -----------------------------------------------------------------
            let scroll_max = (ui.available_height() - 80.0).max(200.0);
            egui::ScrollArea::vertical()
                .max_height(scroll_max)
                .show(ui, |ui| match app.settings_tab {
                    SettingsTab::General => render_general_tab(app, ui, muted),
                    SettingsTab::Appearance => render_appearance_tab(app, ui, ctx, muted),
                    SettingsTab::Skills => render_skills_tab(app, ui, muted),
                    SettingsTab::Tools => render_tools_tab(app, ui, muted),
                });

            // -----------------------------------------------------------------
            // Footer (always visible)
            // -----------------------------------------------------------------
            ui.add_space(24.0);
            ui.separator();

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
                    // Restore theme visuals to match reloaded settings
                    let visuals = match app.settings.theme {
                        deskwork_core::Theme::Dark => egui::Visuals::dark(),
                        deskwork_core::Theme::Light => egui::Visuals::light(),
                    };
                    ctx.set_visuals(visuals);
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

fn render_general_tab(app: &mut DeskworkApp, ui: &mut egui::Ui, muted: egui::Color32) {
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
                    egui::Button::new(RichText::new("Sign in with Claude").size(14.0).strong())
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
                ui.label(RichText::new("Signed in").color(colors::SUCCESS).size(14.0));
            });

            ui.add_space(8.0);

            ui.horizontal(|ui| {
                if ui
                    .add(egui::Button::new("Refresh Models").rounding(Rounding::same(8.0)))
                    .clicked()
                {
                    app.fetch_models();
                }

                if app.fetching_models {
                    ui.spinner();
                }

                if ui
                    .add(
                        egui::Button::new(RichText::new("Sign Out").color(colors::ERROR))
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
                    egui::Button::new(RichText::new("Try Again").size(14.0).strong())
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
        ui.label(RichText::new(model_desc).size(11.0).color(muted).italics());
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
}

fn render_appearance_tab(
    app: &mut DeskworkApp,
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    muted: egui::Color32,
) {
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

    ui.add_space(16.0);

    // Rendering
    ui.heading("Rendering");
    ui.separator();

    ui.horizontal(|ui| {
        ui.label("Mode:");
        ui.add_space(8.0);

        let is_software = app.settings.render_mode == deskwork_core::RenderMode::Software;
        if ui.selectable_label(!is_software, "Auto (GPU)").clicked() && is_software {
            app.settings.render_mode = deskwork_core::RenderMode::Auto;
        }
        if ui.selectable_label(is_software, "Software (CPU)").clicked() && !is_software {
            app.settings.render_mode = deskwork_core::RenderMode::Software;
        }
    });

    ui.label(
        RichText::new(
            "Software mode uses CPU rendering — ideal for terminal servers \
                     and VMs without GPU access. Requires restart.",
        )
        .size(11.0)
        .color(muted)
        .italics(),
    );

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.checkbox(
            &mut app.settings.stream_markdown_enabled,
            "Render streamed markdown",
        );
        ui.label(
            RichText::new("(headings, lists, links, code fences, tables)")
                .size(11.0)
                .color(muted),
        );
    });
}

fn render_tools_tab(app: &mut DeskworkApp, ui: &mut egui::Ui, muted: egui::Color32) {
    // Header with refresh button
    ui.horizontal(|ui| {
        ui.heading("External Tools");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add(egui::Button::new("Refresh").rounding(Rounding::same(8.0)))
                .clicked()
            {
                app.refresh_tool_statuses();
            }
        });
    });
    ui.separator();

    ui.label(
        RichText::new(
            "Download optional tools for advanced features like document conversion and JavaScript execution.",
        )
        .size(11.0)
        .color(muted),
    );

    ui.add_space(8.0);

    // Auto-refresh on first view if empty
    if app.tool_statuses.is_empty() && !app.is_refreshing_tool_statuses() {
        app.refresh_tool_statuses();
    }

    // Loading indicator
    if app.is_refreshing_tool_statuses() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(
                RichText::new("Loading tool status...")
                    .size(12.0)
                    .color(muted),
            );
        });
        ui.add_space(8.0);
    }

    // Tool cards
    let tool_definitions = get_all_tool_definitions();
    for def in &tool_definitions {
        let tool_id = def.id;
        let status = app.tool_statuses.get(&tool_id).cloned();

        egui::Frame::group(ui.style())
            .inner_margin(egui::Margin::same(10.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // Left side: tool info
                    ui.vertical(|ui| {
                        ui.label(RichText::new(def.display_name).strong().size(14.0));
                        ui.label(RichText::new(def.description).size(11.0).color(muted));
                        let required_by = def.required_by.join(", ");
                        ui.label(
                            RichText::new(format!(
                                "~{} MB • Required by: {}",
                                def.size_mb, required_by
                            ))
                            .size(10.0)
                            .color(muted),
                        );
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(status) = status {
                            if status.is_installing {
                                // Show progress
                                ui.vertical(|ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "Installing... {}%",
                                            status.install_progress
                                        ))
                                        .size(11.0)
                                        .color(colors::USER_BG),
                                    );
                                    let progress = status.install_progress as f32 / 100.0;
                                    ui.add(egui::ProgressBar::new(progress).desired_width(120.0));
                                });
                            } else if status.is_installed {
                                // Show installed + uninstall button
                                if ui
                                    .add(
                                        egui::Button::new("Uninstall")
                                            .rounding(Rounding::same(8.0)),
                                    )
                                    .clicked()
                                {
                                    app.start_tool_uninstall(tool_id);
                                }
                                ui.label(
                                    RichText::new(format!(
                                        "Installed v{}",
                                        status.version.as_deref().unwrap_or("?")
                                    ))
                                    .size(11.0)
                                    .color(colors::SUCCESS),
                                );
                            } else if status.is_supported {
                                // Show install button
                                if ui
                                    .add(
                                        egui::Button::new(RichText::new("Install").strong())
                                            .fill(colors::USER_BG)
                                            .rounding(Rounding::same(8.0)),
                                    )
                                    .clicked()
                                {
                                    app.start_tool_install(tool_id);
                                }
                                ui.label(RichText::new("Not Installed").size(11.0).color(muted));
                            } else {
                                ui.label(
                                    RichText::new("Unsupported Platform")
                                        .size(11.0)
                                        .color(colors::ERROR),
                                );
                            }
                        } else {
                            ui.spinner();
                        }
                    });
                });
            });

        ui.add_space(4.0);
    }
}

fn render_skills_tab(app: &mut DeskworkApp, ui: &mut egui::Ui, muted: egui::Color32) {
    // =========================================================================
    // Section 1: Skill Categories (knowledge-work categories with enable/disable)
    // =========================================================================
    ui.heading("Skill Categories");
    ui.separator();

    ui.label(
        RichText::new(
            "Knowledge categories that extend the assistant with domain-specific skills and commands. \
             Enable the categories relevant to your work.",
        )
        .size(11.0)
        .color(muted),
    );

    ui.add_space(8.0);

    // Reload button
    if ui
        .add(egui::Button::new("Reload Categories").rounding(Rounding::same(8.0)))
        .clicked()
    {
        app.reload_categories();
    }

    ui.add_space(8.0);

    // Category list with enable/disable toggles
    let categories = app
        .category_registry
        .all_categories()
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();

    if categories.is_empty() {
        ui.label(
            RichText::new("No skill categories found.")
                .size(12.0)
                .color(muted),
        );
    } else {
        for category in categories {
            let mut enabled = app
                .settings
                .plugins_enabled
                .iter()
                .any(|id| id == &category.id);

            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::same(8.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut enabled, "").changed() {
                            app.set_category_enabled(&category.id, enabled);
                        }

                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(format!("{} ({})", category.name, category.id))
                                    .strong(),
                            );
                            ui.label(
                                RichText::new(format!(
                                    "Status: {:?} | Commands: {} | Skills: {} | Connectors: {}",
                                    category.status,
                                    category.commands.len(),
                                    category.skills.len(),
                                    category.mcp_servers.len(),
                                ))
                                .size(11.0)
                                .color(muted),
                            );
                            if !category.description.is_empty() {
                                ui.label(
                                    RichText::new(&category.description)
                                        .size(11.0)
                                        .color(muted),
                                );
                            }
                        });
                    });

                });

            ui.add_space(4.0);
        }
    }

    // =========================================================================
    // Section 2: Python Tools (skill scripts from skills.zip bundle)
    // =========================================================================
    ui.add_space(16.0);
    ui.heading("Python Tools");
    ui.separator();

    ui.label(
        RichText::new(
            "Python-based skills that extend the assistant's capabilities. \
             Skills are bundled with the application and extracted on first run.",
        )
        .size(11.0)
        .color(muted),
    );

    ui.add_space(8.0);

    // Use cached skills from app.skills_context
    let skills: Vec<deskwork_core::SkillMetadata> = app
        .skills_context
        .as_ref()
        .map(|ctx| ctx.skills.clone())
        .unwrap_or_default();

    if skills.is_empty() {
        ui.label(
            RichText::new(
                "No Python tools found. Tools will be available after the bundle is extracted.",
            )
            .size(12.0)
            .color(muted),
        );
    } else {
        ui.label(
            RichText::new(format!("{} Python tool(s) available", skills.len()))
                .size(12.0)
                .strong(),
        );

        ui.add_space(4.0);

        for skill in &skills {
            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::same(10.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&skill.name).strong().size(14.0));
                            ui.label(RichText::new(&skill.description).size(11.0).color(muted));
                            ui.label(
                                RichText::new(format!("License: {}", skill.license))
                                    .size(10.0)
                                    .color(muted),
                            );
                            ui.label(
                                RichText::new(format!("Path: {}", skill.path.display()))
                                    .size(10.0)
                                    .color(muted),
                            );
                        });
                    });
                });

            ui.add_space(4.0);
        }
    }

    // Show venv info
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(4.0);

    if let Some(ref ctx) = app.skills_context {
        ui.label(RichText::new("Python Environment").strong().size(12.0));

        ui.label(
            RichText::new(format!("Venv: {}", ctx.venv_path.display()))
                .size(10.0)
                .color(muted),
        );
        ui.label(
            RichText::new(format!("Python: {}", ctx.python_path.display()))
                .size(10.0)
                .color(muted),
        );

        if !ctx.installed_tools.is_empty() {
            ui.add_space(4.0);
            ui.label(
                RichText::new("External Tools Available:")
                    .strong()
                    .size(12.0),
            );
            for tool in &ctx.installed_tools {
                ui.label(
                    RichText::new(format!(
                        "  • {} → {}",
                        tool.name,
                        tool.executable_path.display()
                    ))
                    .size(10.0)
                    .color(muted),
                );
            }
        }
    }
}
