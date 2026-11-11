//! The shared file table of the Search and Folders tabs: virtualized rows, sortable headers, multi-selection,
//! double-click and context menu.

use std::collections::BTreeSet;

use egui::{Align, Layout, RichText, Sense};
use egui_extras::{Column, TableBuilder};

use super::app::Fmt;
use crate::core::listing::SortKey;

/// Selected rows (indices into the displayed list).
#[derive(Debug, Default)]
pub(super) struct Selection {
    pub rows: BTreeSet<usize>,
    anchor: Option<usize>,
}

impl Selection {
    pub fn only(&mut self, row: usize) {
        self.rows = BTreeSet::from([row]);
        self.anchor = Some(row);
    }

    pub fn clear(&mut self) {
        self.rows.clear();
        self.anchor = None;
    }

    /// Click with Ctrl (toggle) or Shift (range from the last clicked row) like a file manager.
    fn click(&mut self, row: usize, modifiers: egui::Modifiers) {
        match self.anchor {
            Some(anchor) if modifiers.shift => self.rows = (anchor.min(row)..=anchor.max(row)).collect(),
            _ if modifiers.command => {
                if !self.rows.remove(&row) {
                    self.rows.insert(row);
                }
                self.anchor = Some(row);
            }
            _ => self.only(row),
        }
    }
}

/// An action chosen in the context menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FileAction {
    Open,
    Reveal,
    Properties,
    Copy,
    ExportTree,
}

/// A column of the table.
pub(super) struct Col {
    pub title: &'static str,
    pub key: SortKey,
    pub width: f32,
    pub right: bool,
}

/// What happened in the table this frame.
#[derive(Debug, Default)]
pub(super) struct TableOutput {
    pub double_clicked: Option<usize>,
    pub sort_changed: bool,
    pub action: Option<FileAction>,
}

/// Draws the table. `cell(row, col)` gives a cell's text; `is_dir(row)` and `medium(row)` drive the context menu.
#[allow(clippy::too_many_arguments, reason = "plain drawing helper shared by two tabs")]
pub(super) fn file_table(
    ui: &mut egui::Ui,
    id: &str,
    fmt: &Fmt,
    cols: &[Col],
    sort: &mut (SortKey, bool),
    rows: usize,
    selection: &mut Selection,
    scroll_to: Option<usize>,
    cell: impl Fn(usize, usize) -> String,
    is_dir: impl Fn(usize) -> bool,
    medium: impl Fn(usize) -> bool,
) -> TableOutput {
    let s = fmt.s;
    let mut out = TableOutput::default();
    let modifiers = ui.input(|i| i.modifiers);
    let mut table = TableBuilder::new(ui)
        .id_salt(id)
        .striped(true)
        .resizable(true)
        .sense(Sense::click())
        .auto_shrink([false, false])
        .cell_layout(Layout::left_to_right(Align::Center));
    for (i, col) in cols.iter().enumerate() {
        let column = if i + 1 == cols.len() { Column::remainder() } else { Column::initial(col.width) };
        table = table.column(column.at_least(40.0).clip(true));
    }
    if let Some(row) = scroll_to {
        table = table.scroll_to_row(row, Some(Align::Center));
    }
    table
        .header(22.0, |mut header| {
            for col in cols {
                header.col(|ui| {
                    let arrow = match *sort {
                        (key, true) if key == col.key => " ⏶",
                        (key, false) if key == col.key => " ⏷",
                        _ => "",
                    };
                    let label =
                        egui::Label::new(RichText::new(format!("{}{arrow}", col.title)).strong()).sense(Sense::click());
                    if ui.add(label).clicked() {
                        *sort = if sort.0 == col.key { (col.key, !sort.1) } else { (col.key, true) };
                        out.sort_changed = true;
                    }
                });
            }
        })
        .body(|body| {
            body.rows(20.0, rows, |mut row| {
                let index = row.index();
                row.set_selected(selection.rows.contains(&index));
                for (c, col) in cols.iter().enumerate() {
                    row.col(|ui| {
                        let text = cell(index, c);
                        if col.right {
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.add(egui::Label::new(text).selectable(false))
                            });
                        } else {
                            ui.add(egui::Label::new(text).truncate().selectable(false));
                        }
                    });
                }
                let response = row.response();
                if response.clicked() {
                    selection.click(index, modifiers);
                }
                if response.double_clicked() {
                    out.double_clicked = Some(index);
                }
                if response.secondary_clicked() && !selection.rows.contains(&index) {
                    selection.only(index);
                }
                response.context_menu(|ui| {
                    let available = medium(index);
                    let mut item = |ui: &mut egui::Ui, label: &str, action: FileAction, enabled: bool| {
                        let button = ui.add_enabled(enabled, egui::Button::new(label));
                        let button = if enabled { button } else { button.on_disabled_hover_text(s.needs_medium) };
                        if button.clicked() {
                            out.action = Some(action);
                            ui.close();
                        }
                    };
                    item(ui, s.open, FileAction::Open, available);
                    item(ui, s.show_in_explorer, FileAction::Reveal, available);
                    if crate::platform::HAS_PROPERTIES {
                        item(ui, s.properties, FileAction::Properties, available);
                    }
                    ui.separator();
                    item(ui, s.copy, FileAction::Copy, true);
                    if is_dir(index) {
                        item(ui, s.export_tree, FileAction::ExportTree, true);
                    }
                });
            });
        });
    out
}
