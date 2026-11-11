//! The shared file table of the Search and Folders tabs: virtualized rows, sortable headers, multi-selection,
//! double-click and context menu.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

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
    Copy(CopyWhat),
    ExportTree,
}

/// What "Copy" puts in the clipboard, one line per selected entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CopyWhat {
    Name,
    FullPath,
    ParentPath,
    Row,
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

/// Draws the table. `cell(row, col)` gives a cell's text, the first column being the name, drawn after a folder or
/// file icon; `is_dir(row)` picks that icon and, with `medium(row)`, drives the context menu.
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
                        if c == 0 {
                            entry_icon(ui, is_dir(index), &text);
                        }
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
                    ui.menu_button(s.copy, |ui| {
                        for (label, what) in [
                            (s.copy_name, CopyWhat::Name),
                            (s.copy_full_path, CopyWhat::FullPath),
                            (s.copy_parent_path, CopyWhat::ParentPath),
                            (s.copy_row, CopyWhat::Row),
                        ] {
                            item(ui, label, FileAction::Copy(what), true);
                        }
                    });
                    if is_dir(index) {
                        item(ui, s.export_tree, FileAction::ExportTree, true);
                    }
                });
            });
        });
    out
}

/// Folder or file icon in front of an entry name: the OS icon of the file type when there is one.
fn entry_icon(ui: &mut egui::Ui, is_dir: bool, name: &str) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(16.0, 16.0), Sense::hover());
    if is_dir {
        paint_folder(ui.painter(), rect, false);
        return;
    }
    let ext = Path::new(name).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if let Some(texture) = file_type_icon(ui.ctx(), &ext) {
        let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        ui.painter().image(texture.id(), rect, uv, egui::Color32::WHITE);
    } else {
        let color = ui.visuals().text_color();
        let font = egui::TextStyle::Body.resolve(ui.style());
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "📄", font, color);
    }
}

/// OS icons already loaded, by lowercase extension (`None`: the OS has none).
type IconCache = HashMap<String, Option<egui::TextureHandle>>;

/// The OS icon of files with extension `ext`, asked for once per extension and kept in egui's memory.
fn file_type_icon(ctx: &egui::Context, ext: &str) -> Option<egui::TextureHandle> {
    let id = egui::Id::unique("file_type_icons");
    if let Some(cached) = ctx.data_mut(|d| d.get_temp_mut_or_default::<IconCache>(id).get(ext).cloned()) {
        return cached;
    }
    let name = if ext.is_empty() { "file".to_owned() } else { format!("file.{ext}") };
    // The 32 px icon stays sharp on high-DPI screens; the 16 px one is drawn pixel for pixel otherwise.
    let large = ctx.pixels_per_point() > 1.25;
    let texture = crate::platform::file_icon(&name, large).map(|icon| {
        let image = egui::ColorImage::from_rgba_unmultiplied(icon.size, &icon.rgba);
        ctx.load_texture(format!("file-icon-{ext}"), image, egui::TextureOptions::LINEAR)
    });
    ctx.data_mut(|d| d.get_temp_mut_or_default::<IconCache>(id).insert(ext.to_owned(), texture.clone()));
    texture
}

/// Folder icon colors, the classic manila yellow (readable on light and dark backgrounds): body, the
/// darker back of an open folder, and the outline.
const FOLDER_COLOR: egui::Color32 = egui::Color32::from_rgb(0xF2, 0xC2, 0x4E);
const FOLDER_BACK: egui::Color32 = egui::Color32::from_rgb(0xC9, 0x93, 0x2A);
const FOLDER_EDGE: egui::Color32 = egui::Color32::from_rgb(0x8A, 0x62, 0x14);

/// Paints a small folder icon centered in `rect`, open (front flap tilted down) or closed.
pub(super) fn paint_folder(painter: &egui::Painter, rect: egui::Rect, open: bool) {
    let c = rect.center();
    let p = |x: f32, y: f32| egui::pos2(c.x + x, c.y + y);
    let edge = egui::Stroke::new(1.0, FOLDER_EDGE);
    let tab = egui::Rect::from_min_max(p(-7.0, -6.0), p(-1.5, -3.5));
    let body = egui::Rect::from_min_max(p(-7.0, -4.0), p(7.0, 5.5));
    painter.rect(tab, 1.0, if open { FOLDER_BACK } else { FOLDER_COLOR }, edge, egui::StrokeKind::Middle);
    painter.rect(body, 1.0, if open { FOLDER_BACK } else { FOLDER_COLOR }, edge, egui::StrokeKind::Middle);
    if open {
        let flap = vec![p(-4.5, -1.0), p(8.5, -1.0), p(6.5, 5.5), p(-7.0, 5.5)];
        painter.add(egui::Shape::convex_polygon(flap, FOLDER_COLOR, edge));
    }
}
