//! Graphical interface (egui). A thin layer: the logic lives in `core`, `catalog` and `search`.

mod add_dialog;
mod app;
mod folders_tab;
mod list;
mod search_tab;
mod sidebar;
mod windows;

use crate::catalog::store::CatalogDir;
use crate::config::Settings;

/// Opens the main window and runs until it is closed.
///
/// # Errors
/// When the window or its graphics context cannot be created.
pub fn run(dir: CatalogDir, settings: Settings, warning: Option<String>) -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Oxycat")
            .with_inner_size([1180.0, 720.0])
            .with_min_inner_size([760.0, 420.0])
            .with_drag_and_drop(true),
        ..eframe::NativeOptions::default()
    };
    eframe::run_native(
        "Oxycat",
        options,
        Box::new(move |cc| Ok(Box::new(app::OxycatApp::new(&cc.egui_ctx, dir, settings, warning)))),
    )
}
#[cfg(test)]
mod tests;
