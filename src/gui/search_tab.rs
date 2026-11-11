//! Search tab: the form, its filters, the background search and the results table.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use chrono::Local;

use super::app::OxycatApp;
use super::folders_tab::Target;
use super::list::{Col, Selection, file_table};
use crate::catalog::store::CatalogInfo;
use crate::core::form::{FormError, SearchForm, parse_form};
use crate::core::listing::{SortKey, selection_total, sort_hits};
use crate::core::model::{Hit, KindFilter};
use crate::core::modes::SearchMode;
use crate::i18n::{Strings, fill};
use crate::search::{SearchResult, search};

/// State of the Search tab.
pub(super) struct SearchTab {
    form: SearchForm,
    show_filters: bool,
    error: Option<FormError>,
    pub hits: Vec<Hit>,
    truncated: bool,
    errors: Vec<(String, String)>,
    elapsed: Option<Duration>,
    searched: usize,
    limit: usize,
    running: Option<Receiver<SearchResult>>,
    sort: (SortKey, bool),
    pub selection: Selection,
}

impl Default for SearchTab {
    fn default() -> Self {
        SearchTab {
            form: SearchForm::default(),
            show_filters: false,
            error: None,
            hits: Vec::new(),
            truncated: false,
            errors: Vec::new(),
            elapsed: None,
            searched: 0,
            limit: 0,
            running: None,
            sort: (SortKey::Name, true),
            selection: Selection::default(),
        }
    }
}

impl SearchTab {
    /// Picks up the result of the running search, if it is done.
    pub fn poll(&mut self) {
        let Some(result) = self.running.as_ref().and_then(|rx| rx.try_recv().ok()) else {
            return;
        };
        self.running = None;
        self.hits = result.hits;
        sort_hits(&mut self.hits, self.sort.0, self.sort.1);
        self.truncated = result.truncated;
        self.errors = result.errors;
        self.elapsed = Some(result.elapsed);
        self.selection.clear();
    }

    /// The selected hits, as file-action targets.
    pub fn targets(&self) -> Vec<Target> {
        self.selection
            .rows
            .iter()
            .filter_map(|&i| self.hits.get(i))
            .map(|h| Target { file: h.catalog_file.clone(), parent_path: h.path.clone(), entry: h.entry.clone() })
            .collect()
    }
}

/// Label of a search mode.
fn mode_label(s: &Strings, mode: SearchMode) -> &'static str {
    match mode {
        SearchMode::Standard => s.mode_standard,
        SearchMode::Duplicates => s.mode_duplicates,
        SearchMode::SameNames => s.mode_same_names,
        SearchMode::SameSizes => s.mode_same_sizes,
        SearchMode::Singles => s.mode_singles,
        SearchMode::Parents => s.mode_parents,
        SearchMode::NotArchived => s.mode_not_archived,
        SearchMode::Archived => s.mode_archived,
    }
}

/// Label of the form field holding an invalid value.
fn field_label(s: &Strings, error: FormError) -> String {
    match error {
        FormError::Pattern => s.too_many_wildcards.to_owned(),
        FormError::From => format!("{} {} : {}", s.date, s.from, s.invalid_value),
        FormError::To => format!("{} {} : {}", s.date, s.to, s.invalid_value),
        FormError::MinSize => format!("{} {} : {}", s.size, s.min, s.invalid_value),
        FormError::MaxSize => format!("{} {} : {}", s.size, s.max, s.invalid_value),
    }
}

impl OxycatApp {
    pub(super) fn search_ui(&mut self, ui: &mut egui::Ui) {
        let s = self.fmt.s;
        let mut run = false;
        let enter = |r: &egui::Response, ui: &egui::Ui| r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let bar = egui::vec2(ui.available_width(), 24.0);
        ui.allocate_ui_with_layout(bar, egui::Layout::right_to_left(egui::Align::Center), |ui| {
            run |= ui.button(s.search).clicked();
            let arrow = if self.search.show_filters { "⏶" } else { "⏷" };
            if ui.button(format!("{} {arrow}", s.filters)).clicked() {
                self.search.show_filters = !self.search.show_filters;
            }
            let field = egui::TextEdit::singleline(&mut self.search.form.pattern)
                .hint_text(s.search_hint)
                .desired_width(ui.available_width());
            let response = ui.add(field);
            run |= enter(&response, ui);
        });
        if self.search.show_filters {
            let form = &mut self.search.form;
            ui.horizontal_wrapped(|ui| {
                let mut small = |ui: &mut egui::Ui, text: &mut String, hint: &str| {
                    let response = ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(90.0));
                    run |= enter(&response, ui);
                };
                ui.label(s.date);
                small(ui, &mut form.from, s.from);
                small(ui, &mut form.to, s.to);
                ui.separator();
                ui.label(s.size);
                small(ui, &mut form.min_size, s.min);
                small(ui, &mut form.max_size, s.max);
                ui.separator();
                egui::ComboBox::from_label(s.kind)
                    .selected_text(match form.kind {
                        KindFilter::All => s.kind_all,
                        KindFilter::Files => s.kind_files,
                        KindFilter::Dirs => s.kind_dirs,
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut form.kind, KindFilter::All, s.kind_all);
                        ui.selectable_value(&mut form.kind, KindFilter::Files, s.kind_files);
                        ui.selectable_value(&mut form.kind, KindFilter::Dirs, s.kind_dirs);
                    });
                egui::ComboBox::from_label(s.mode).selected_text(mode_label(s, form.mode)).show_ui(ui, |ui| {
                    for mode in SearchMode::ALL {
                        ui.selectable_value(&mut form.mode, mode, mode_label(s, mode));
                    }
                });
                if ui.button(s.reset).clicked() {
                    *form = SearchForm { pattern: std::mem::take(&mut form.pattern), ..SearchForm::default() };
                }
            });
        }
        if let Some(error) = self.search.error {
            ui.colored_label(ui.visuals().error_fg_color, format!("⚠ {}", field_label(s, error)));
        }
        if run {
            self.run_search();
        }
        if self.search.running.is_some() {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(s.searching);
            });
        }
        ui.add_space(4.0);
        self.results_table(ui);
    }

    fn results_table(&mut self, ui: &mut egui::Ui) {
        let s = self.fmt.s;
        let cols = [
            Col { title: s.col_name, key: SortKey::Name, width: 320.0, right: false },
            Col { title: s.col_size, key: SortKey::Size, width: 90.0, right: true },
            Col { title: s.col_date, key: SortKey::Date, width: 130.0, right: false },
            Col { title: s.col_path, key: SortKey::Path, width: 260.0, right: false },
            Col { title: s.col_catalog, key: SortKey::Catalog, width: 90.0, right: false },
        ];
        let available: HashMap<PathBuf, bool> =
            self.listing.catalogs.iter().map(|c| (c.file.clone(), self.locate(&c.volume).is_some())).collect();
        let fmt = &self.fmt;
        let SearchTab { hits, selection, sort, .. } = &mut self.search;
        let out = file_table(
            ui,
            "search_results",
            fmt,
            &cols,
            sort,
            hits.len(),
            selection,
            None,
            |i, c| {
                let hit = &hits[i];
                match c {
                    0 => format!("{} {}", if hit.entry.is_dir { "📁" } else { "📄" }, hit.entry.name),
                    1 => hit.entry.size.map_or_else(|| "—".to_owned(), |b| fmt.size(b)),
                    2 => fmt.date(hit.entry.mtime),
                    3 => hit.path.clone(),
                    _ => hit.catalog_name.clone(),
                }
            },
            |i| hits[i].entry.is_dir,
            |i| available.get(&hits[i].catalog_file).copied().unwrap_or(false),
        );
        if out.sort_changed {
            sort_hits(&mut self.search.hits, self.search.sort.0, self.search.sort.1);
            self.search.selection.clear();
        }
        if let Some(i) = out.double_clicked {
            let hit = self.search.hits[i].clone();
            self.open_folder(&hit.catalog_file, hit.entry.parent_id, Some(hit.entry.id));
        }
        if let Some(action) = out.action {
            let targets = self.search.targets();
            self.file_action(action, &targets);
        }
    }

    /// Validates the form and starts the search on a background thread.
    fn run_search(&mut self) {
        let today = Local::now().date_naive();
        let query = match parse_form(&self.search.form, today, &Local) {
            Ok(query) => query,
            Err(error) => {
                self.search.error = Some(error);
                return;
            }
        };
        self.search.error = None;
        let catalogs: Vec<CatalogInfo> =
            self.listing.catalogs.iter().filter(|c| self.checked.contains(&c.file)).cloned().collect();
        if catalogs.is_empty() {
            self.banner = Some(self.fmt.s.no_catalog_checked.to_owned());
            return;
        }
        let (mode, limit) = (self.search.form.mode, self.settings.hit_limit.max(1));
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        self.search.searched = catalogs.len();
        self.search.limit = limit;
        let spawned = std::thread::Builder::new().name("oxycat-search".to_owned()).spawn(move || {
            let _ = tx.send(search(&catalogs, &query, mode, limit));
            ctx.request_repaint();
        });
        // A newer search replaces the receiver: results of an older one are simply dropped.
        self.search.running = spawned.ok().map(|_| rx);
    }

    pub(super) fn search_status(&mut self, ui: &mut egui::Ui) {
        let s = self.fmt.s;
        let st = &self.search;
        ui.horizontal(|ui| {
            ui.label(fill(s.results, &[("n", &self.fmt.int(st.hits.len() as u64))]));
            ui.label(fill(s.catalogs_searched, &[("n", &st.searched.to_string())]));
            if let Some(elapsed) = st.elapsed {
                ui.label(format!("{:.2} s", elapsed.as_secs_f64()));
            }
            let (n, size) =
                selection_total(st.selection.rows.iter().filter_map(|&i| st.hits.get(i)).map(|h| h.entry.size));
            if n > 0 {
                ui.label(fill(s.selection, &[("n", &n.to_string()), ("size", &self.fmt.size(size))]));
            }
            if st.truncated {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    fill(s.limit_reached, &[("n", &self.fmt.int(st.limit as u64))]),
                );
            }
            if !st.errors.is_empty() {
                let names = st.errors.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>().join(", ");
                let details = st.errors.iter().map(|(n, e)| format!("{n}: {e}")).collect::<Vec<_>>().join("\n");
                ui.colored_label(ui.visuals().error_fg_color, fill(s.search_errors, &[("names", &names)]))
                    .on_hover_text(details);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.weak(s.double_click_hint));
        });
    }
}
