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

pub(super) fn harness(dir: CatalogDir) -> Harness<'static, OxycatApp> {
    let settings = Settings { language: Some(Lang::En), ..Settings::default() };
    Harness::builder()
        .with_size([1200.0, 800.0])
        .with_step_dt(1.0 / 60.0)
        .build_eframe(move |cc| OxycatApp::new(&cc.egui_ctx, dir, settings, None))
}

/// Steps the interface until `done` holds (background threads deliver results asynchronously).
pub(super) fn step_until(harness: &mut Harness<'static, OxycatApp>, done: impl Fn(&OxycatApp) -> bool) {
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
    assert!(shows(&harness, "Astérix"));

    // Double-click the result (after a pause, or egui counts a triple click with the Search click): the Folders
    // tab opens on its parent (BD) with the result selected.
    harness.run_steps(30);
    for _ in 0..2 {
        harness.query_all_by(|n| n.value().is_some_and(|v| v == "Astérix")).next().unwrap().click();
        harness.step();
    }
    harness.run_steps(3);
    assert_eq!(harness.state().tab, Tab::Folders);
    assert_eq!(harness.state().folders.selection.rows.len(), 1);
    assert!(shows(&harness, "Tintin.cbz"));

    // Backspace goes up to the top of the source.
    harness.key_press(egui::Key::Backspace);
    harness.run_steps(3);
    assert!(harness.query_by_role_and_label(Role::Label, "BD").is_some());
    assert!(!shows(&harness, "Tintin.cbz"));
}

/// Whether some widget displays exactly `text`.
pub(super) fn shows(harness: &Harness<'static, OxycatApp>, text: &str) -> bool {
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

/// Text the interface put in the clipboard during the last frame.
fn copied(harness: &Harness<'static, OxycatApp>) -> Option<String> {
    harness.output().platform_output.commands.iter().find_map(|c| match c {
        egui::OutputCommand::CopyText(text) => Some(text.clone()),
        _ => None,
    })
}

#[test]
fn keyboard_copy_of_path_and_name() {
    let (tmp, info) = catalog_of("B1", &[("BD/Tintin.cbz", Some(4))]);
    let mut harness = harness(CatalogDir::new(tmp.path().to_path_buf()));
    harness.run_steps(2);
    let conn = crate::catalog::store::open_read(&info.file).unwrap();
    let bd = crate::catalog::query::children(&conn, None).unwrap().remove(0);
    let tintin = crate::catalog::query::children(&conn, Some(bd.id)).unwrap().remove(0);
    drop(conn);
    harness.state_mut().open_folder(&info.file, Some(bd.id), Some(tintin.id));
    harness.run_steps(2);

    harness.event(egui::Event::Copy);
    harness.step();
    let expected = std::path::Path::new(&info.volume.source_path).join("BD").join("Tintin.cbz");
    assert_eq!(copied(&harness), Some(expected.to_string_lossy().into_owned()));

    let modifiers = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
    let press = egui::Event::Key { key: egui::Key::C, physical_key: None, pressed: true, repeat: false, modifiers };
    harness.event(egui::Event::ModifiersChanged(modifiers));
    harness.event(press);
    harness.step();
    assert_eq!(copied(&harness).as_deref(), Some("Tintin.cbz"));
}

#[test]
fn folders_tab_follows_catalog_clicks_and_panel_collapses() {
    let (tmp, a) = catalog_of("Alpha", &[("a.txt", Some(1))]);
    let (tmp_b, b) = catalog_of("Beta", &[("b.txt", Some(1))]);
    std::fs::copy(&b.file, tmp.path().join("Beta.db")).unwrap();
    drop(tmp_b);
    let mut harness = harness(CatalogDir::new(tmp.path().to_path_buf()));
    harness.run_steps(2);
    harness.state_mut().open_folder(&a.file, None, None);
    harness.run_steps(2);

    harness.get_by_label("Beta").click();
    harness.run_steps(2);
    let browsed = harness.state().folders.catalog.as_ref().map(|c| c.volume.name.clone());
    assert_eq!(browsed.as_deref(), Some("Beta"));

    harness.get_all_by_label("◀").next().unwrap().click();
    harness.run_steps(2);
    assert!(harness.state().settings.catalogs_collapsed);
    assert!(harness.query_by_label("Catalogs").is_none());
    assert!(harness.query_by_label("Alpha").is_some());
    harness.get_all_by_label("▶").next().unwrap().click();
    harness.run_steps(2);
    assert!(harness.query_by_label("Catalogs").is_some());
}

#[test]
fn filter_labels_are_left_of_their_combo_boxes() {
    let tmp = tempfile::tempdir().unwrap();
    let mut harness = harness(CatalogDir::new(tmp.path().to_path_buf()));
    harness.run_steps(2);
    harness.get_by_label_contains("Filters").click();
    harness.run_steps(2);
    for label in ["Type", "Mode"] {
        let text = harness.get_by_role_and_label(Role::Label, label).rect();
        let combo = harness.get_by_role_and_label(Role::ComboBox, label).rect();
        assert!(text.max.x <= combo.min.x, "{label}: label {text:?} should be left of {combo:?}");
    }
}

/// Whether the folder tree shows a node named `name` (list cells are plain labels, tree nodes are buttons).
fn tree_shows(harness: &Harness<'static, OxycatApp>, name: &str) -> bool {
    harness.query_by_role_and_label(Role::Button, name).is_some()
}

#[test]
fn folder_tree_browses_and_expands() {
    let (tmp, info) = catalog_of("B1", &[("BD/Astérix/01.cbz", Some(3)), ("BD/Tintin.cbz", Some(4)), ("Films", None)]);
    let mut harness = harness(CatalogDir::new(tmp.path().to_path_buf()));
    harness.run_steps(2);
    harness.state_mut().open_folder(&info.file, None, None);
    harness.run_steps(2);
    // The root is expanded: its sub-folders are listed, theirs are not yet.
    assert!(tree_shows(&harness, "Films"));
    assert!(!tree_shows(&harness, "Astérix"));

    // A click on a tree node shows that folder and expands its branch.
    harness.get_by_role_and_label(Role::Button, "BD").click();
    harness.run_steps(2);
    assert!(shows(&harness, "Tintin.cbz"));
    assert!(tree_shows(&harness, "Astérix"));

    // The arrow collapses it again without leaving the folder.
    harness.get_all_by_label("⏷").nth(1).unwrap().click();
    harness.run_steps(2);
    assert!(!tree_shows(&harness, "Astérix"));
    assert!(shows(&harness, "Tintin.cbz"));
}

#[test]
fn double_click_on_missing_file_explains() {
    let (tmp, info) = catalog_of("B1", &[("notes.txt", Some(4))]);
    // The file is gone from the (still mounted) medium: opening must not launch anything but say why. Only the file
    // goes: without a volume serial (Linux CI), the source folder's presence is what says the medium is plugged in.
    std::fs::remove_file(tmp.path().join("src").join("notes.txt")).unwrap();
    let mut harness = harness(CatalogDir::new(tmp.path().to_path_buf()));
    // The mount watcher thread reports the medium asynchronously.
    step_until(&mut harness, |app| app.locate(&info.volume).is_some());
    harness.state_mut().open_folder(&info.file, None, None);
    harness.run_steps(2);
    for _ in 0..2 {
        harness.get_by_role_and_label(Role::Label, "notes.txt").click();
        harness.step();
    }
    harness.run_steps(2);
    let banner = harness.state().banner.clone().unwrap_or_default();
    assert!(banner.contains("Not found on the plugged-in medium"), "{banner:?}");
}

#[test]
fn unusable_catalog_dir_is_reported_at_startup() {
    let tmp = tempfile::tempdir().unwrap();
    let blocker = tmp.path().join("file");
    std::fs::write(&blocker, b"not a directory").unwrap();
    let harness = harness(CatalogDir::new(blocker.join("catalogs")));
    let banner = harness.state().banner.clone().unwrap_or_default();
    assert!(banner.contains("Cannot create the catalog directory"), "{banner:?}");
}

#[test]
fn icon_decodes() {
    let icon = super::icon();
    assert_eq!((icon.width, icon.height), (256, 256));
}
