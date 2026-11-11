//! Headless end-to-end checks of the interface with `egui_kittest` (no window, no desktop interaction).

use std::time::{Duration, Instant};

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use super::app::{OxycatApp, Tab};
use crate::catalog::query::tests::catalog_of;
use crate::catalog::store::CatalogDir;
use crate::config::Settings;
use crate::i18n::Lang;

fn harness(dir: CatalogDir) -> Harness<'static, OxycatApp> {
    let settings = Settings { language: Some(Lang::En), ..Settings::default() };
    Harness::builder()
        .with_size([1200.0, 800.0])
        .with_step_dt(1.0 / 60.0)
        .build_eframe(move |cc| OxycatApp::new(&cc.egui_ctx, dir, settings, None))
}

/// Steps the interface until `done` holds (background threads deliver results asynchronously).
fn step_until(harness: &mut Harness<'static, OxycatApp>, done: impl Fn(&OxycatApp) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done(harness.state()) {
        assert!(Instant::now() < deadline, "timed out");
        harness.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    harness.run_steps(3);
}

#[test]
fn search_then_open_folder() {
    let (tmp, _info) = catalog_of("B1", &[("BD/Astérix/01.cbz", Some(3)), ("BD/Tintin.cbz", Some(4))]);
    let mut harness = harness(CatalogDir::new(tmp.path().to_path_buf()));
    harness.run_steps(3);
    assert!(harness.query_by_label("B1").is_some());

    let fields = harness.get_all_by_role(Role::TextInput).collect::<Vec<_>>();
    // The search field is the widest text input: the last one laid out in the central panel.
    let search_field = fields.last().unwrap();
    search_field.focus();
    search_field.type_text("asterix");
    harness.run_steps(2);
    harness.get_by_label("Search").click();
    step_until(&mut harness, |app| !app.search.hits.is_empty());
    // Accent-insensitive: "asterix" finds the "Astérix" folder.
    assert_eq!(harness.state().search.hits.len(), 1);
    assert!(shows(&harness, "📁 Astérix"));

    // Double-click the result (after a pause, or egui counts a triple click with the Search click): the Folders
    // tab opens on its parent (BD) with the result selected.
    harness.run_steps(30);
    for _ in 0..2 {
        harness.query_all_by(|n| n.value().is_some_and(|v| v == "📁 Astérix")).next().unwrap().click();
        harness.step();
    }
    harness.run_steps(3);
    assert_eq!(harness.state().tab, Tab::Folders);
    assert_eq!(harness.state().folders.selection.rows.len(), 1);
    assert!(shows(&harness, "📄 Tintin.cbz"));

    // Backspace goes up to the top of the source.
    harness.key_press(egui::Key::Backspace);
    harness.run_steps(3);
    assert!(shows(&harness, "📁 BD"));
    assert!(!shows(&harness, "📄 Tintin.cbz"));
}

/// Whether some widget displays exactly `text`.
fn shows(harness: &Harness<'static, OxycatApp>, text: &str) -> bool {
    harness.query_all_by(|n| n.value().is_some_and(|v| v == text)).next().is_some()
}

#[test]
fn language_and_theme_are_saved() {
    let tmp = tempfile::tempdir().unwrap();
    let mut harness = harness(CatalogDir::new(tmp.path().to_path_buf()));
    harness.run_steps(2);
    harness.get_by_label("🌙 Dark").click();
    harness.run_steps(2);
    let (saved, _) = crate::config::load_settings(tmp.path());
    assert_eq!(saved.theme, crate::config::ThemeChoice::Dark);
    assert_eq!(saved.language, Some(Lang::En));
}
