//! Deskwork GUI Application
//!
//! A Claude-powered coding assistant with a native desktop interface.

mod app;
mod ui;

use eframe::egui;

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

    // Create tokio runtime for async operations
    let runtime = tokio::runtime::Runtime::new().expect("Failed to create tokio runtime");
    let _guard = runtime.enter();

    // Window configuration
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([800.0, 600.0])
            .with_title("Deskwork - Claude Coding Assistant"),
        ..Default::default()
    };

    // Run the application
    eframe::run_native(
        "Deskwork",
        options,
        Box::new(|cc| Ok(Box::new(app::DeskworkApp::new(cc, runtime)))),
    )
}
