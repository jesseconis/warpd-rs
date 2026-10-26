mod app;
mod schema;
mod validation;

use crate::config::LoadedConfig;
use eframe::egui;

pub fn run_config_editor(initial: LoadedConfig) -> anyhow::Result<()> {
    let title = "warpd-rs config";
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(title)
            .with_inner_size([980.0, 760.0])
            .with_min_inner_size([900.0, 640.0]),
        ..Default::default()
    };

    eframe::run_native(
        title,
        native_options,
        Box::new(move |cc| Ok(Box::new(app::ConfigEditorApp::new(cc, initial.clone())))),
    )
    .map_err(|error| anyhow::anyhow!("failed to launch configuration editor: {error}"))?;

    Ok(())
}
