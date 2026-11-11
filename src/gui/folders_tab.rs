//! Folders tab: browsing one catalog, keyboard navigation, and the file actions shared with the Search tab.

use std::collections::HashSet;
use std::path::{MAIN_SEPARATOR, MAIN_SEPARATOR_STR, PathBuf};
use std::time::SystemTime;

use super::app::{OxycatApp, Tab};
use super::list::{Col, CopyWhat, FileAction, Selection, file_table, paint_folder};
use crate::catalog::query::{ancestors, children, dir_tree, subtree_dirs};
use crate::catalog::store::{CatalogInfo, open_read};
use crate::core::listing::{
    DirTree, NEWLINE, SortKey, TreeRow, filter_catalogs, rows_to_tsv, selection_total, sort_entries, tree_text,
    visible_tree,
};
use crate::core::locate::{entry_path, join_stored};
use crate::core::model::Entry;
use crate::i18n::fill;
use crate::platform;

/// An entry a file action applies to.
pub(super) struct Target {
    pub file: PathBuf,
    /// Parent folder path from the cataloged source, with a leading separator.
    pub parent_path: String,
    pub entry: Entry,
}

/// State of the Folders tab.
pub(super) struct FoldersTab {
    pub catalog: Option<CatalogInfo>,
    dir: Option<i64>,
    crumbs: Vec<Entry>,
    rows: Vec<Entry>,
    pub selection: Selection,
    sort: (SortKey, bool),
    scroll_to: Option<usize>,
    /// Folder tree of the catalog, and the catalog file and modification time it was read from.
    tree: DirTree,
    tree_of: Option<(PathBuf, SystemTime)>,
    /// Expanded folders of the tree (`None` = the root).
    expanded: HashSet<Option<i64>>,
    /// Scroll the tree to the current folder on the next frame.
    tree_scroll: bool,
}

/// A navigation requested during drawing, applied afterwards.
#[derive(Clone, Copy)]
enum Nav {
    Dir(Option<i64>),
    Catalog(isize),
}

impl FoldersTab {
    pub fn empty() -> FoldersTab {
        FoldersTab {
            catalog: None,
            dir: None,
            crumbs: Vec::new(),
            rows: Vec::new(),
            selection: Selection::default(),
            sort: (SortKey::Name, true),
            scroll_to: None,
            tree: DirTree::new(),
            tree_of: None,
            expanded: HashSet::new(),
            tree_scroll: false,
        }
    }

    /// Loads a folder (`None` = top of the source), selecting and scrolling to `select` if given.
    pub fn load(&mut self, info: CatalogInfo, dir: Option<i64>, select: Option<i64>) -> anyhow::Result<()> {
        let conn = open_read(&info.file)?;
        let mut rows = children(&conn, dir)?;
        sort_entries(&mut rows, self.sort.0, self.sort.1);
        self.crumbs = dir.map(|d| ancestors(&conn, d)).transpose()?.unwrap_or_default();
        // The whole tree is read once per catalog version, so moving between folders stays instant.
        let version = Some((info.file.clone(), info.modified));
        if self.tree_of != version {
            self.tree = dir_tree(&conn)?;
            self.tree_of = version;
            self.expanded = HashSet::from([None]);
        }
        drop(conn);
        self.expanded.extend(self.crumbs.iter().map(|e| Some(e.id)));
        self.tree_scroll = true;
        self.rows = rows;
        self.dir = dir;
        self.catalog = Some(info);
        self.selection.clear();
        self.scroll_to = None;
        if let Some(pos) = select.and_then(|id| self.rows.iter().position(|e| e.id == id)) {
            self.selection.only(pos);
            self.scroll_to = Some(pos);
        }
        Ok(())
    }

    /// Path of the current folder from the cataloged source, with a leading separator.
    fn path(&self) -> String {
        let names: Vec<&str> = self.crumbs.iter().map(|e| e.name.as_str()).collect();
        format!("{MAIN_SEPARATOR}{}", names.join(MAIN_SEPARATOR_STR))
    }

    pub fn targets(&self) -> Vec<Target> {
        let Some(info) = &self.catalog else {
            return Vec::new();
        };
        let parent_path = self.path();
        self.selection
            .rows
            .iter()
            .filter_map(|&i| self.rows.get(i))
            .map(|e| Target { file: info.file.clone(), parent_path: parent_path.clone(), entry: e.clone() })
            .collect()
    }
}

impl OxycatApp {
    pub(super) fn folders_ui(&mut self, ui: &mut egui::Ui) {
        let s = self.fmt.s;
        let Some(info) = self.folders.catalog.clone() else {
            ui.weak(s.choose_catalog);
            return;
        };
        let mut nav = None;
        ui.horizontal(|ui| {
            if ui.button("◀").on_hover_text(s.prev_catalog).clicked() {
                nav = Some(Nav::Catalog(-1));
            }
            if ui.button("▶").on_hover_text(s.next_catalog).clicked() {
                nav = Some(Nav::Catalog(1));
            }
            let parent = self.folders.crumbs.len().checked_sub(2).map(|i| self.folders.crumbs[i].id);
            if ui.add_enabled(self.folders.dir.is_some(), egui::Button::new(s.up)).clicked() {
                nav = Some(Nav::Dir(parent));
            }
            if ui.link(&info.volume.name).clicked() {
                nav = Some(Nav::Dir(None));
            }
            for crumb in &self.folders.crumbs {
                ui.label("›");
                if ui.link(&crumb.name).clicked() {
                    nav = Some(Nav::Dir(Some(crumb.id)));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.weak(s.keys_hint));
        });
        ui.add_space(4.0);

        egui::Panel::left("folder_tree")
            .resizable(true)
            .default_size(260.0)
            .min_size(120.0)
            .show(ui, |ui| self.tree_ui(ui, &info.volume.name, &mut nav));
        egui::CentralPanel::default().show(ui, |ui| self.rows_ui(ui, &info, &mut nav));
        if let Some(nav) = nav {
            self.navigate(nav);
        }
    }

    /// Folder tree of the catalog: a click shows a folder, the arrow expands or collapses it.
    #[allow(clippy::cast_precision_loss, reason = "row counts and depths are far below f32 precision limits")]
    fn tree_ui(&mut self, ui: &mut egui::Ui, root: &str, nav: &mut Option<Nav>) {
        let FoldersTab { tree, expanded, dir, tree_scroll, .. } = &mut self.folders;
        let rows = visible_tree(root, tree, expanded);
        let row_height = ui.spacing().interact_size.y;
        let mut area = egui::ScrollArea::both().id_salt("folder_tree_scroll").auto_shrink(false);
        if std::mem::take(tree_scroll)
            && let Some(i) = rows.iter().position(|r| r.id == *dir)
        {
            let row_step = row_height + ui.spacing().item_spacing.y;
            area = area.vertical_scroll_offset((i as f32 * row_step - ui.available_height() / 2.0).max(0.0));
        }
        let mut toggle = None;
        area.show_rows(ui, row_height, rows.len(), |ui, range| {
            for row in &rows[range] {
                let open = expanded.contains(&row.id);
                let inner = ui.horizontal(|ui| {
                    ui.set_height(row_height);
                    ui.add_space(row.follows.len() as f32 * TREE_INDENT);
                    let arrow = [row_height * 0.8, row_height];
                    let arrow_rect = if row.has_children {
                        let icon = if open { "⏷" } else { "⏵" };
                        let response = ui.add_sized(arrow, egui::Button::new(icon).frame(false));
                        if response.clicked() {
                            toggle = Some(row.id);
                        }
                        response.rect
                    } else {
                        ui.allocate_exact_size(arrow.into(), egui::Sense::hover()).0
                    };
                    let current = row.id == *dir;
                    let (icon_rect, icon) = ui.allocate_exact_size(egui::vec2(16.0, row_height), egui::Sense::click());
                    paint_folder(ui.painter(), icon_rect, current || (open && row.has_children));
                    let icon_clicked = icon.clicked();
                    if (ui.selectable_label(current, row.name).clicked() || icon_clicked) && !current {
                        *nav = Some(Nav::Dir(row.id));
                    }
                    arrow_rect
                });
                paint_connectors(ui, row, open, inner.response.rect, inner.inner);
            }
        });
        if let Some(id) = toggle
            && !expanded.remove(&id)
        {
            expanded.insert(id);
        }
    }

    /// Entries of the current folder.
    fn rows_ui(&mut self, ui: &mut egui::Ui, info: &CatalogInfo, nav: &mut Option<Nav>) {
        let s = self.fmt.s;
        let cols = [
            Col { title: s.col_name, key: SortKey::Name, width: 420.0, right: false },
            Col { title: s.col_size, key: SortKey::Size, width: 100.0, right: true },
            Col { title: s.col_date, key: SortKey::Date, width: 140.0, right: false },
        ];
        let medium = self.locate(&info.volume).is_some();
        let fmt = &self.fmt;
        let FoldersTab { rows, selection, sort, scroll_to, .. } = &mut self.folders;
        let out = file_table(
            ui,
            "folder_rows",
            fmt,
            &cols,
            sort,
            rows.len(),
            selection,
            scroll_to.take(),
            |i, c| {
                let e = &rows[i];
                match c {
                    0 => e.name.clone(),
                    1 => e.size.map_or_else(|| "—".to_owned(), |b| fmt.size(b)),
                    _ => fmt.date(e.mtime),
                }
            },
            |i| rows[i].is_dir,
            |_| medium,
        );
        if out.sort_changed {
            sort_entries(&mut self.folders.rows, self.folders.sort.0, self.folders.sort.1);
            self.folders.selection.clear();
        }
        if let Some(i) = out.double_clicked
            && self.folders.rows[i].is_dir
        {
            *nav = Some(Nav::Dir(Some(self.folders.rows[i].id)));
        }
        if let Some(action) = out.action {
            let targets = self.folders.targets();
            self.file_action(action, &targets);
        }
    }

    fn navigate(&mut self, nav: Nav) {
        let Some(info) = self.folders.catalog.clone() else {
            return;
        };
        match nav {
            Nav::Dir(dir) => {
                // Going up selects the folder we came from.
                let came_from = self.folders.dir;
                self.open_folder(&info.file, dir, came_from);
            }
            Nav::Catalog(step) => {
                let shown: Vec<PathBuf> =
                    filter_catalogs(&self.listing.catalogs, &self.filter).iter().map(|c| c.file.clone()).collect();
                let here = shown.iter().position(|f| *f == info.file);
                let next = here.and_then(|i| i.checked_add_signed(step)).and_then(|i| shown.get(i)).cloned();
                if let Some(file) = next {
                    self.selected = Some(file.clone());
                    self.open_folder(&file, None, None);
                }
            }
        }
    }

    pub(super) fn folders_status(&mut self, ui: &mut egui::Ui) {
        let s = self.fmt.s;
        let Some(info) = &self.folders.catalog else {
            return;
        };
        let rows = &self.folders.rows;
        ui.horizontal(|ui| {
            let (n, total) = selection_total(rows.iter().map(|e| e.size));
            ui.label(fill(s.elements, &[("n", &self.fmt.int(n as u64)), ("size", &self.fmt.size(total))]));
            let (n, size) =
                selection_total(self.folders.selection.rows.iter().filter_map(|&i| rows.get(i)).map(|e| e.size));
            if n > 0 {
                ui.label(fill(s.selection, &[("n", &n.to_string()), ("size", &self.fmt.size(size))]));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match self.locate(&info.volume) {
                Some(path) => ui.label(fill(s.mounted_on, &[("path", &path.to_string_lossy())])),
                None => ui.weak(s.not_mounted),
            });
        });
    }

    /// Keyboard shortcuts, ignored while a text field has the focus.
    pub(super) fn handle_keys(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || self.add.is_some() || self.pending.is_some() {
            return;
        }
        let (copy, copy_name, back, next, prev) = ctx.input(|i| {
            (
                i.events.iter().any(|e| matches!(e, egui::Event::Copy)),
                i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::C),
                i.key_pressed(egui::Key::Backspace),
                i.modifiers.command && i.key_pressed(egui::Key::N),
                i.modifiers.command && i.key_pressed(egui::Key::P),
            )
        });
        if copy || copy_name {
            let targets = if self.tab == Tab::Search { self.search.targets() } else { self.folders.targets() };
            let what = if copy_name { CopyWhat::Name } else { CopyWhat::FullPath };
            self.copy(&targets, what);
        }
        if self.tab == Tab::Folders && self.folders.catalog.is_some() {
            if back && self.folders.dir.is_some() {
                let parent = self.folders.crumbs.len().checked_sub(2).map(|i| self.folders.crumbs[i].id);
                self.navigate(Nav::Dir(parent));
            } else if next {
                self.navigate(Nav::Catalog(1));
            } else if prev {
                self.navigate(Nav::Catalog(-1));
            }
        }
    }

    /// Runs a context-menu action on the selected entries.
    pub(super) fn file_action(&mut self, action: FileAction, targets: &[Target]) {
        let s = self.fmt.s;
        let result = match action {
            FileAction::Copy(what) => {
                self.copy(targets, what);
                Ok(())
            }
            FileAction::ExportTree => targets.first().map_or(Ok(()), |t| self.export_tree(t)),
            FileAction::Open | FileAction::Reveal | FileAction::Properties => {
                let Some(target) = targets.first() else {
                    return;
                };
                let real = self
                    .catalog(&target.file)
                    .and_then(|c| self.locate(&c.volume))
                    .map(|root| entry_path(&root, &target.parent_path, &target.entry.name));
                match real {
                    None => Err(anyhow::anyhow!(s.needs_medium)),
                    Some(path) if action == FileAction::Open => platform::open(&path),
                    Some(path) if action == FileAction::Reveal => platform::reveal(&path),
                    Some(path) => platform::show_properties(&path),
                }
            }
        };
        if let Err(e) = result {
            self.banner = Some(fill(s.err_other, &[("error", &format!("{e:#}"))]));
        }
    }

    /// Copies the selected entries, one line each.
    fn copy(&self, targets: &[Target], what: CopyWhat) {
        if targets.is_empty() {
            return;
        }
        let text = if what == CopyWhat::Row {
            let rows: Vec<Vec<String>> = targets
                .iter()
                .map(|t| {
                    let catalog = self.catalog(&t.file).map(|c| c.volume.name.clone()).unwrap_or_default();
                    vec![
                        t.entry.name.clone(),
                        t.entry.size.map(|b| b.to_string()).unwrap_or_default(),
                        self.fmt.date(t.entry.mtime),
                        t.parent_path.clone(),
                        catalog,
                    ]
                })
                .collect();
            rows_to_tsv(&rows)
        } else {
            let line = |t: &Target| {
                if what == CopyWhat::Name {
                    return t.entry.name.clone();
                }
                // Real location when the medium is plugged in, else where it was cataloged from.
                let root = self
                    .catalog(&t.file)
                    .map(|c| self.locate(&c.volume).unwrap_or_else(|| PathBuf::from(&c.volume.source_path)));
                let parent = join_stored(&root.unwrap_or_default(), &t.parent_path);
                let path = if what == CopyWhat::FullPath { parent.join(&t.entry.name) } else { parent };
                path.to_string_lossy().into_owned()
            };
            targets.iter().map(line).collect::<Vec<_>>().join(NEWLINE)
        };
        self.ctx.copy_text(text);
    }

    fn export_tree(&self, target: &Target) -> anyhow::Result<()> {
        let conn = open_read(&target.file)?;
        let dirs = subtree_dirs(&conn, Some(target.entry.id))?;
        drop(conn);
        let catalog = self.catalog(&target.file).map(|c| c.volume.name.clone()).unwrap_or_default();
        let sep = if target.parent_path.ends_with(MAIN_SEPARATOR) { String::new() } else { MAIN_SEPARATOR.to_string() };
        let root = format!("{catalog}:{}{sep}{}", target.parent_path, target.entry.name);
        let file_name = format!("{}.txt", target.entry.name);
        if let Some(path) = rfd::FileDialog::new().set_file_name(file_name).add_filter("txt", &["txt"]).save_file() {
            std::fs::write(path, tree_text(&root, &dirs))?;
        }
        Ok(())
    }
}

/// Horizontal step between two levels of the folder tree.
const TREE_INDENT: f32 = 16.0;

/// Draws the dotted lines tying a tree row to its parent and siblings, like the classic Windows tree view.
#[allow(clippy::cast_precision_loss, reason = "depths are far below f32 precision limits")]
fn paint_connectors(ui: &egui::Ui, row: &TreeRow<'_>, open: bool, row_rect: egui::Rect, arrow: egui::Rect) {
    let half_gap = ui.spacing().item_spacing.y / 2.0;
    let (top, bottom, mid) = (row_rect.top() - half_gap, row_rect.bottom() + half_gap, arrow.center().y);
    // A dot spacing that divides the row step, so the dots line up from one row to the next.
    let step = bottom - top;
    let spacing = step / (step / 3.0).round().max(1.0);
    let depth = row.follows.len();
    let column = |level: usize| arrow.center().x - (depth - level) as f32 * TREE_INDENT;
    let vertical = |x: f32, from: f32, to: f32| [egui::pos2(x, from), egui::pos2(x, to)];
    let glyph = arrow.width() / 3.0;
    let mut lines = Vec::new();
    if let Some((&more_below, outer)) = row.follows.split_last() {
        // Lines of the ancestors that still have siblings further down, passing by.
        for (level, _) in outer.iter().enumerate().filter(|&(_, &follows)| follows) {
            lines.push(vertical(column(level), top, bottom));
        }
        // This folder's own branch, continuing down when a sibling follows.
        let x = column(depth - 1);
        lines.push(vertical(x, top, if more_below { bottom } else { mid }));
        let end = if row.has_children { arrow.center().x - glyph } else { arrow.right() };
        lines.push([egui::pos2(x, mid), egui::pos2(end, mid)]);
    }
    if open && row.has_children {
        lines.push(vertical(arrow.center().x, mid + glyph, bottom));
    }
    let color = ui.visuals().weak_text_color();
    for line in lines {
        ui.painter().extend(egui::Shape::dotted_line(&line, color, spacing, 0.8));
    }
}
