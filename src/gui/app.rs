//! Main window state and frame layout.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use chrono::Local;

use super::add_dialog::{AddDialog, ScanJob};
use super::folders_tab::FoldersTab;
use super::search_tab::SearchTab;
use super::windows::SettingsDraft;
use crate::catalog::store::{CatalogDir, CatalogInfo, Listing};
use crate::config::{Settings, ThemeChoice, save_settings};
use crate::core::listing::format_timestamp;
use crate::core::locate::{is_network_path, locate_source};
use crate::core::model::Volume;
use crate::core::units::{NumberFormat, format_int, format_size};
use crate::i18n::{Lang, Strings, date_format, detect, fill, number_format, strings};
use crate::platform;

/// The two tabs of the right-hand side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Tab {
    Search,
    Folders,
}

/// Language-dependent formatting, rebuilt when the language or the settings change.
pub(super) struct Fmt {
    pub s: &'static Strings,
    nf: NumberFormat,
    date_pattern: String,
}

impl Fmt {
    fn new(lang: Lang, settings: &Settings) -> Fmt {
        let mut nf = number_format(lang);
        if let Some(sep) = settings.thousands_separator.as_ref().filter(|s| !s.is_empty()) {
            nf.thousands = sep.clone().into();
        }
        let date_pattern =
            settings.date_format.clone().filter(|f| !f.is_empty()).unwrap_or(date_format(lang).to_owned());
        Fmt { s: strings(lang), nf, date_pattern }
    }

    pub fn size(&self, bytes: u64) -> String {
        format_size(bytes, &self.nf)
    }

    pub fn int(&self, n: u64) -> String {
        format_int(n, &self.nf)
    }

    pub fn date(&self, ts: Option<i64>) -> String {
        ts.map(|t| format_timestamp(t, &self.date_pattern, &Local)).unwrap_or_default()
    }
}

/// A question waiting for the user's answer.
pub(super) enum Pending {
    Delete { file: PathBuf, name: String },
    Rename { file: PathBuf, draft: String },
    Overwrite(AddDialog),
}

/// What a background thread knows about plugged media; drives and network shares can answer slowly, so the
/// interface thread never asks them itself.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Media {
    /// Mounted volumes: root and serial number.
    volumes: Vec<(PathBuf, Option<String>)>,
    /// Source paths to check by existence (network shares, media without serial number).
    probe: Vec<PathBuf>,
    /// Those of `probe` found reachable at the last check.
    reachable: HashSet<PathBuf>,
}

/// Shared with the media watcher thread.
pub(super) type Mounted = Arc<Mutex<Media>>;

/// The application.
pub struct OxycatApp {
    pub(super) dir: CatalogDir,
    pub(super) settings: Settings,
    pub(super) lang: Lang,
    pub(super) fmt: Fmt,
    pub(super) listing: Listing,
    fingerprint: Vec<(PathBuf, Option<SystemTime>, u64)>,
    last_poll: Instant,
    known: HashSet<PathBuf>,
    pub(super) checked: HashSet<PathBuf>,
    pub(super) selected: Option<PathBuf>,
    pub(super) filter: String,
    /// Text fields of the details card being edited: catalog file, comment, ignore patterns.
    pub(super) card_draft: Option<(PathBuf, String, String)>,
    pub(super) tab: Tab,
    pub(super) banner: Option<String>,
    pub(super) search: SearchTab,
    pub(super) folders: FoldersTab,
    pub(super) add: Option<AddDialog>,
    pub(super) job: Option<ScanJob>,
    pub(super) pending: Option<Pending>,
    pub(super) settings_window: Option<SettingsDraft>,
    pub(super) about_open: bool,
    pub(super) mounted: Mounted,
    /// Kept to wake the interface up from background threads.
    pub(super) ctx: egui::Context,
}

impl OxycatApp {
    pub(super) fn new(ctx: &egui::Context, dir: CatalogDir, settings: Settings, warning: Option<String>) -> OxycatApp {
        let lang = settings.language.unwrap_or_else(|| detect(sys_locale::get_locale().as_deref()));
        let fmt = Fmt::new(lang, &settings);
        let banner = warning.map(|w| fill(fmt.s.settings_invalid, &[("warning", &w)]));
        let mounted: Mounted = Arc::default();
        let mut app = OxycatApp {
            dir,
            settings,
            lang,
            fmt,
            listing: Listing::default(),
            fingerprint: Vec::new(),
            last_poll: Instant::now(),
            known: HashSet::new(),
            checked: HashSet::new(),
            selected: None,
            filter: String::new(),
            card_draft: None,
            tab: Tab::Search,
            banner,
            search: SearchTab::default(),
            folders: FoldersTab::default(),
            add: None,
            job: None,
            pending: None,
            settings_window: None,
            about_open: false,
            mounted,
            ctx: ctx.clone(),
        };
        app.apply_theme(ctx);
        app.reload();
        // Started once `reload` has filled `probe`: its first pass then already checks serial-less media, instead of
        // reporting them unplugged until the next pass 3 s later.
        spawn_mount_watcher(Arc::clone(&app.mounted), ctx.clone());
        app
    }

    /// Re-reads the catalog directory; new catalogs start checked.
    pub(super) fn reload(&mut self) {
        self.listing = self.dir.list();
        self.fingerprint = fingerprint(self.dir.root());
        for c in &self.listing.catalogs {
            if self.known.insert(c.file.clone()) {
                self.checked.insert(c.file.clone());
            }
        }
        let probe: Vec<PathBuf> = self
            .listing
            .catalogs
            .iter()
            .filter(|c| c.volume.serial.is_none() || is_network_path(&c.volume.source_path))
            .map(|c| PathBuf::from(&c.volume.source_path))
            .collect();
        if let Ok(mut media) = self.mounted.lock() {
            media.probe = probe;
        }
        let present: HashSet<&PathBuf> = self.listing.catalogs.iter().map(|c| &c.file).collect();
        self.checked.retain(|f| present.contains(f));
        if self.selected.as_ref().is_some_and(|f| !present.contains(f)) {
            self.selected = None;
        }
    }

    /// Reloads when files were added, removed or changed in the catalog directory (checked every 2 s).
    fn poll_catalogs(&mut self) {
        if self.last_poll.elapsed() < Duration::from_secs(2) {
            return;
        }
        self.last_poll = Instant::now();
        if fingerprint(self.dir.root()) != self.fingerprint {
            self.reload();
        }
    }

    pub(super) fn catalog(&self, file: &Path) -> Option<&CatalogInfo> {
        self.listing.catalogs.iter().find(|c| c.file == file)
    }

    /// Where the medium of a catalog is mounted now, if it is.
    pub(super) fn locate(&self, volume: &Volume) -> Option<PathBuf> {
        let Ok(media) = self.mounted.lock() else {
            return None;
        };
        locate_source(volume, &media.volumes, |p| media.reachable.contains(p))
    }

    pub(super) fn set_language(&mut self, language: Option<Lang>) {
        self.settings.language = language;
        self.lang = language.unwrap_or_else(|| detect(sys_locale::get_locale().as_deref()));
        self.fmt = Fmt::new(self.lang, &self.settings);
        self.save_settings();
    }

    pub(super) fn apply_settings(&mut self, settings: Settings, ctx: &egui::Context) {
        self.settings = settings;
        self.fmt = Fmt::new(self.lang, &self.settings);
        self.apply_theme(ctx);
        self.save_settings();
    }

    fn apply_theme(&self, ctx: &egui::Context) {
        ctx.set_theme(match self.settings.theme {
            ThemeChoice::Light => egui::ThemePreference::Light,
            ThemeChoice::Dark => egui::ThemePreference::Dark,
            ThemeChoice::Auto => egui::ThemePreference::System,
        });
    }

    pub(super) fn save_settings(&mut self) {
        if let Err(e) = save_settings(self.dir.root(), &self.settings) {
            self.banner = Some(fill(self.fmt.s.err_other, &[("error", &e.to_string())]));
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let s = self.fmt.s;
        ui.horizontal(|ui| {
            ui.strong("Oxycat");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(s.about).clicked() {
                    self.about_open = true;
                }
                if ui.button(s.settings).clicked() {
                    self.settings_window = Some(SettingsDraft::from(&self.settings));
                }
                ui.separator();
                let mut language = self.settings.language;
                egui::ComboBox::from_id_salt("language")
                    .selected_text(format!("🌐 {}", self.lang.native_name()))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut language, None, s.language_default);
                        for lang in Lang::ALL {
                            ui.selectable_value(&mut language, Some(lang), lang.native_name());
                        }
                    })
                    .response
                    .widget_info(|| egui::WidgetInfo::labeled(egui::accesskit::Role::ComboBox, true, "Language"));
                if language != self.settings.language {
                    self.set_language(language);
                }
                let mut theme = self.settings.theme;
                for (choice, label) in [
                    (ThemeChoice::Auto, s.theme_auto),
                    (ThemeChoice::Dark, s.theme_dark),
                    (ThemeChoice::Light, s.theme_light),
                ] {
                    ui.selectable_value(&mut theme, choice, label);
                }
                if theme != self.settings.theme {
                    let settings = Settings { theme, ..self.settings.clone() };
                    self.apply_settings(settings, ui.ctx());
                }
            });
        });
    }

    fn banner(&mut self, ui: &mut egui::Ui) {
        let Some(text) = self.banner.clone() else {
            return;
        };
        ui.horizontal(|ui| {
            ui.colored_label(ui.visuals().warn_fg_color, text);
            if ui.small_button("✖").clicked() {
                self.banner = None;
            }
        });
    }

    fn handle_drop(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()));
        if let Some(path) = dropped.filter(|p| p.is_dir())
            && self.job.is_none()
        {
            self.add = Some(AddDialog::for_source(&path, &self.settings));
        }
    }
}

impl eframe::App for OxycatApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_catalogs();
        self.poll_job();
        self.search.poll();
        self.handle_drop(&ctx);
        self.handle_keys(&ctx);

        egui::Panel::top("top").show(ui, |ui| {
            self.top_bar(ui);
            self.banner(ui);
        });
        egui::Panel::bottom("status").show(ui, |ui| match self.tab {
            Tab::Search => self.search_status(ui),
            Tab::Folders => self.folders_status(ui),
        });
        if self.settings.catalogs_collapsed {
            let width = self.compact_width(ui);
            egui::Panel::left("catalogs_compact")
                .resizable(false)
                .exact_size(width)
                .show(ui, |ui| self.sidebar_compact(ui));
        } else {
            egui::Panel::left("catalogs")
                .resizable(true)
                .default_size(260.0)
                .min_size(200.0)
                .show(ui, |ui| self.sidebar(ui));
        }
        egui::CentralPanel::default().show(ui, |ui| {
            let s = self.fmt.s;
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.tab, Tab::Search, s.tab_search);
                let open = self.tab == Tab::Folders;
                let icon = egui::IdSalt::new("folders_tab_icon");
                let tab =
                    egui::Button::selectable(open, (egui::Atom::custom(icon, egui::vec2(16.0, 14.0)), s.tab_folders))
                        .atom_ui(ui);
                if let Some(rect) = tab.rect(icon) {
                    super::list::paint_folder(ui.painter(), rect, open);
                }
                if tab.clicked() {
                    self.tab = Tab::Folders;
                }
            });
            ui.separator();
            match self.tab {
                Tab::Search => self.search_ui(ui),
                Tab::Folders => self.folders_ui(ui),
            }
        });
        self.dialogs(&ctx);
        self.windows(&ctx);
        ctx.request_repaint_after(Duration::from_secs(2));
    }
}

/// Name, modification time and size of each `.db` file: a cheap way to notice catalog changes.
fn fingerprint(root: &Path) -> Vec<(PathBuf, Option<SystemTime>, u64)> {
    let mut out: Vec<_> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("db")))
        .map(|e| {
            let meta = e.metadata().ok();
            (e.path(), meta.as_ref().and_then(|m| m.modified().ok()), meta.map_or(0, |m| m.len()))
        })
        .collect();
    out.sort();
    out
}

/// Refreshes the mounted volumes and the reachability of network sources every 3 s on a background thread.
fn spawn_mount_watcher(mounted: Mounted, ctx: egui::Context) {
    std::thread::Builder::new()
        .name("oxycat-mounts".to_owned())
        .spawn(move || {
            loop {
                let probe = mounted.lock().map(|m| m.probe.clone()).unwrap_or_default();
                let volumes = platform::mounted_volumes();
                let reachable: HashSet<PathBuf> = probe.iter().filter(|p| p.exists()).cloned().collect();
                let changed = mounted.lock().is_ok_and(|mut m| {
                    let changed = m.volumes != volumes || m.reachable != reachable;
                    m.volumes = volumes;
                    m.reachable = reachable;
                    changed
                });
                if changed {
                    ctx.request_repaint();
                }
                std::thread::sleep(Duration::from_secs(3));
            }
        })
        .ok();
}
