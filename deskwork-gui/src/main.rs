//! Deskwork GUI Application
//!
//! A Claude-powered coding assistant with a native desktop interface.

mod app;
mod ui;

use eframe::egui;
use deskwork_core::{Database, RenderMode, Settings};

fn main() -> eframe::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("deskwork=debug".parse().unwrap())
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    tracing::info!("Starting Deskwork v{}", deskwork_core::VERSION);

    // Load settings early to determine render mode before window creation.
    // This must happen before eframe::run_native() since hardware acceleration
    // is set at window creation time.
    let render_mode = load_render_mode();
    let hardware_acceleration = match render_mode {
        RenderMode::Auto => eframe::HardwareAcceleration::Preferred,
        RenderMode::Software => eframe::HardwareAcceleration::Off,
    };
    tracing::info!(?render_mode, "Render mode configured");

    // Create tokio runtime for async operations
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let _guard = runtime.enter();

    // Window configuration
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("Deskwork - Claude Coding Assistant"),
        hardware_acceleration,
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };

    // Run the application
    eframe::run_native(
        "Deskwork",
        options,
        Box::new(|cc| Ok(Box::new(app::DeskworkApp::new(cc, runtime)))),
    )
}

/// Load only the render mode from persisted settings.
///
/// Opens a temporary DB connection just to read the setting, then drops it.
/// This runs before the eframe window is created so we can configure
/// hardware acceleration at startup.
fn load_render_mode() -> RenderMode {
    match Database::open() {
        Ok(db) => {
            if let Err(e) = db.migrate() {
                tracing::warn!("Failed to migrate DB for render mode: {e}");
            }
            Settings::load(&db).render_mode
        }
        Err(e) => {
            tracing::warn!("Failed to open DB for render mode: {e}, using Auto");
            RenderMode::Auto
        }
    }
}
