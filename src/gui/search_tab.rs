//! Search tab: the form, its filters, the background search and the results table.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::Duration;

use chrono::Local;

use super::app::OxycatApp;
use super::folders_tab::Target;
use super::list::{Col, Selection, file_table};
use crate::catalog::store::CatalogInfo;
use crate::core::form::{FormError, SearchForm, parse_form};
use crate::core::listing::{SortKey, ids_by_catalog, selection_total, sort_hits};
use crate::core::model::{Hit, KindFilter};
use crate::core::modes::SearchMode;
use crate::core::pattern::PatternError;
use crate::i18n::{Strings, fill};
use crate::search::{SearchResult, search};

/// What the search thread sends back.
pub(super) struct Done {
    result: SearchResult,
    /// The catalogs searched, as they were then.
    catalogs: Vec<CatalogInfo>,
    /// For a search within results: size of the results it started from, and whether those were incomplete.
    base: Option<(usize, bool)>,
}

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
    running: Option<Receiver<Done>>,
    sort: (SortKey, bool),
    pub selection: Selection,
    /// The "Search within results" checkbox.
    pub within: bool,
    /// Entry ids of the hits, by catalog file.
    ids: HashMap<PathBuf, Vec<i64>>,
    /// The catalogs of the hits, as they were when searched: ids are stale once one of them changed.
    sources: Vec<CatalogInfo>,
    /// See [`Done::base`], for the displayed hits.
    base: Option<(usize, bool)>,
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
            within: false,
            ids: HashMap::new(),
            sources: Vec::new(),
            base: None,
        }
    }
}

impl SearchTab {
    /// Picks up the result of the running search, if it is done.
    ///
    /// # Errors
    /// When the search thread stopped without a result (it panicked): the search is over, with no hits.
    pub fn poll(&mut self) -> Result<(), &'static str> {
        let Some(rx) = &self.running else {
            return Ok(());
        };
        let Done { result, catalogs, base } = match rx.try_recv() {
            Ok(done) => done,
            Err(TryRecvError::Empty) => return Ok(()),
            Err(TryRecvError::Disconnected) => {
                self.running = None;
                return Err("search thread stopped");
            }
        };
        self.running = None;
        self.hits = result.hits;
        sort_hits(&mut self.hits, self.sort.0, self.sort.1);
        self.truncated = result.truncated;
        self.errors = result.errors;
        self.elapsed = Some(result.elapsed);
        self.selection.clear();
        self.ids = ids_by_catalog(&self.hits);
        self.sources = catalogs.into_iter().filter(|c| self.ids.contains_key(&c.file)).collect();
        self.base = base;
        Ok(())
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
        FormError::Pattern(error) => match error {
            PatternError::TooManyWildcards => s.too_many_wildcards,
            PatternError::UnbalancedParentheses => s.unbalanced_parentheses,
            PatternError::MissingOperand => s.missing_operand,
            PatternError::MissingOperator => s.missing_operator,
            PatternError::UnterminatedQuote => s.unterminated_quote,
        }
        .to_owned(),
        FormError::From => format!("{} {} : {}", s.date, s.from, s.invalid_value),
        FormError::To => format!("{} {} : {}", s.date, s.to, s.invalid_value),
        FormError::MinSize => format!("{} {} : {}", s.size, s.min, s.invalid_value),
        FormError::MaxSize => format!("{} {} : {}", s.size, s.max, s.invalid_value),
    }
}

impl OxycatApp {
    /// Whether the hits can be searched within: there are some, and none of their catalogs changed since (an update
    /// renumbers the ids, a rename or delete moves the file; any write is treated as a change, to stay safe).
    pub(super) fn can_refine(&self) -> bool {
        let st = &self.search;
        !st.hits.is_empty() && st.sources.iter().all(|c| self.listing.catalogs.contains(c))
    }

    pub(super) fn search_ui(&mut self, ui: &mut egui::Ui) {
        let s = self.fmt.s;
        let mut run = false;
        let refinable = self.can_refine();
        self.search.within &= refinable;
        let idle = self.search.running.is_none();
        let enter = |r: &egui::Response, ui: &egui::Ui| r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let bar = egui::vec2(ui.available_width(), 24.0);
        ui.allocate_ui_with_layout(bar, egui::Layout::right_to_left(egui::Align::Center), |ui| {
            run |= ui.button(s.search).clicked();
            ui.add_enabled(refinable && idle, egui::Checkbox::new(&mut self.search.within, s.within_results));
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
                // Labels go before their combo box (`ComboBox::from_label` puts them after).
                let named = |label: &'static str| {
                    move || egui::WidgetInfo::labeled(egui::accesskit::Role::ComboBox, true, label)
                };
                ui.label(s.kind);
                egui::ComboBox::from_id_salt("kind")
                    .selected_text(match form.kind {
                        KindFilter::All => s.kind_all,
                        KindFilter::Files => s.kind_files,
                        KindFilter::Dirs => s.kind_dirs,
                    })
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut form.kind, KindFilter::All, s.kind_all);
                        ui.selectable_value(&mut form.kind, KindFilter::Files, s.kind_files);
                        ui.selectable_value(&mut form.kind, KindFilter::Dirs, s.kind_dirs);
                    })
                    .response
                    .widget_info(named(s.kind));
                ui.label(s.mode);
                egui::ComboBox::from_id_salt("mode")
                    .selected_text(mode_label(s, form.mode))
                    .show_ui(ui, |ui| {
                        for mode in SearchMode::ALL {
                            ui.selectable_value(&mut form.mode, mode, mode_label(s, mode));
                        }
                    })
                    .response
                    .widget_info(named(s.mode));
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
                    0 => hit.entry.name.clone(),
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
        let st = &self.search;
        let refine = st.within && self.can_refine();
        let within = refine.then(|| st.ids.clone());
        let incomplete = st.truncated || st.base.is_some_and(|(_, incomplete)| incomplete);
        let base = refine.then_some((st.hits.len(), incomplete));
        let (tx, rx) = channel();
        let ctx = self.ctx.clone();
        self.search.searched = catalogs.len();
        self.search.limit = limit;
        let spawned = std::thread::Builder::new().name("oxycat-search".to_owned()).spawn(move || {
            let result = search(&catalogs, &query, mode, limit, within.as_ref());
            let _ = tx.send(Done { result, catalogs, base });
            ctx.request_repaint();
        });
        // A newer search replaces the receiver: results of an older one are simply dropped.
        match spawned {
            Ok(_) => self.search.running = Some(rx),
            Err(e) => self.banner = Some(fill(self.fmt.s.err_other, &[("error", &e.to_string())])),
        }
    }

    pub(super) fn search_status(&mut self, ui: &mut egui::Ui) {
        let s = self.fmt.s;
        let st = &self.search;
        ui.horizontal(|ui| {
            let n = self.fmt.int(st.hits.len() as u64);
            match st.base {
                Some((total, _)) => {
                    ui.label(fill(s.results_within, &[("n", &n), ("total", &self.fmt.int(total as u64))]))
                }
                None => ui.label(fill(s.results, &[("n", &n)])),
            };
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
            if st.base.is_some_and(|(_, incomplete)| incomplete) {
                ui.colored_label(ui.visuals().warn_fg_color, s.within_truncated);
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

#[cfg(test)]
mod tests {
    use std::sync::mpsc::channel;

    use egui::accesskit::Role;
    use egui_kittest::kittest::Queryable;

    use crate::catalog::query::tests::catalog_of;
    use crate::catalog::scan::{ScanControl, ScanOptions, catalog};
    use crate::catalog::store::CatalogDir;
    use crate::gui::tests::{harness, shows, step_until};

    /// Types `pattern` in the search field and runs the search.
    fn search_for(harness: &mut egui_kittest::Harness<'static, crate::gui::app::OxycatApp>, pattern: &str) {
        harness.state_mut().search.form.pattern = pattern.to_owned();
        harness.run_steps(2);
        harness.get_by_role_and_label(Role::Button, "Search").click();
        harness.step();
        step_until(harness, |app| app.search.running.is_none());
    }

    fn names(harness: &egui_kittest::Harness<'static, crate::gui::app::OxycatApp>) -> Vec<String> {
        let mut names: Vec<String> = harness.state().search.hits.iter().map(|h| h.entry.name.clone()).collect();
        names.sort();
        names
    }

    #[test]
    fn search_within_results() {
        let files = [("a1.cbz", Some(1)), ("a2.cbz", Some(1)), ("b1.cbz", Some(1)), ("a3.txt", Some(1))];
        let (tmp, info) = catalog_of("B1", &files);
        let mut harness = harness(CatalogDir::new(tmp.path().to_path_buf()));
        harness.run_steps(2);
        // Nothing to search within yet.
        assert!(!harness.state().can_refine());

        search_for(&mut harness, ".cbz|");
        assert_eq!(names(&harness), ["a1.cbz", "a2.cbz", "b1.cbz"]);
        harness.get_by_label("Search within results").click();
        harness.run_steps(2);
        assert!(harness.state().search.within);
        search_for(&mut harness, "a");
        assert_eq!(names(&harness), ["a1.cbz", "a2.cbz"]);
        assert!(shows(&harness, "2 results within 3"));
        // Chained: still checked, narrows again.
        search_for(&mut harness, "2");
        assert_eq!(names(&harness), ["a2.cbz"]);
        assert!(shows(&harness, "1 results within 2"));

        // Unchecked: the whole catalog again.
        harness.get_by_label("Search within results").click();
        harness.run_steps(2);
        search_for(&mut harness, "a");
        assert_eq!(names(&harness), ["a1.cbz", "a2.cbz", "a3.txt"]);
        assert!(shows(&harness, "3 results"));

        // Updating the catalog renumbers its ids: refining is no longer possible.
        harness.get_by_label("Search within results").click();
        harness.run_steps(2);
        let dir = CatalogDir::new(tmp.path().to_path_buf());
        let opts = ScanOptions { name: info.volume.name.clone(), overwrite: true, ..ScanOptions::default() };
        catalog(&dir, &tmp.path().join("src"), &opts, &ScanControl::default()).unwrap();
        harness.state_mut().reload();
        harness.run_steps(2);
        assert!(!harness.state().can_refine());
        assert!(!harness.state().search.within);
    }

    #[test]
    fn renamed_catalog_stops_refining_and_truncation_warns() {
        let (tmp, info) = catalog_of("B1", &[("a1.cbz", Some(1)), ("a2.cbz", Some(1)), ("a3.cbz", Some(1))]);
        let mut harness = harness(CatalogDir::new(tmp.path().to_path_buf()));
        harness.state_mut().settings.hit_limit = 2;
        harness.run_steps(2);
        search_for(&mut harness, "a");
        assert!(harness.state().search.truncated);
        harness.get_by_label("Search within results").click();
        harness.run_steps(2);
        search_for(&mut harness, "cbz");
        assert_eq!(harness.state().search.hits.len(), 2);
        assert!(shows(&harness, "Searched within truncated results: some matches may be missing"));

        CatalogDir::new(tmp.path().to_path_buf()).rename(&info.file, "B2").unwrap();
        harness.state_mut().reload();
        harness.run_steps(2);
        assert!(!harness.state().can_refine());
    }

    #[test]
    fn lost_search_thread_ends_the_search() {
        let tmp = tempfile::tempdir().unwrap();
        let mut harness = harness(CatalogDir::new(tmp.path().to_path_buf()));
        // The search thread died (it panicked) before sending its result.
        let (tx, rx) = channel();
        drop(tx);
        harness.state_mut().search.running = Some(rx);
        harness.run_steps(2);
        assert!(harness.state().search.running.is_none());
        let banner = harness.state().banner.clone().unwrap_or_default();
        assert!(banner.contains("search thread stopped"), "{banner:?}");
    }
}
