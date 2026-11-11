//! Left panel: catalog list, filter, check boxes and the selected catalog's details card.

use std::path::{Path, PathBuf};

use super::app::{OxycatApp, Pending};
use super::folders_tab::FoldersTab;
use crate::catalog::scan::ScanOptions;
use crate::catalog::store::MetaUpdate;
use crate::core::ignore::IgnoreRules;
use crate::core::listing::filter_catalogs;
use crate::i18n::fill;

/// Height kept for the details card under the list.
const CARD_HEIGHT: f32 = 250.0;

impl OxycatApp {
    pub(super) fn sidebar(&mut self, ui: &mut egui::Ui) {
        let s = self.fmt.s;
        ui.horizontal(|ui| {
            ui.heading(s.catalogs);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_enabled(self.job.is_none(), egui::Button::new(s.add)).clicked() {
                    self.add = Some(super::add_dialog::AddDialog::empty(&self.settings));
                }
            });
        });
        ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text(s.filter_hint).desired_width(f32::INFINITY));
        let shown: Vec<(PathBuf, String, bool, bool)> = filter_catalogs(&self.listing.catalogs, &self.filter)
            .into_iter()
            .map(|c| (c.file.clone(), c.volume.name.clone(), c.volume.archive, c.volume.preserved))
            .collect();
        ui.horizontal(|ui| {
            if ui.small_button(s.check_all).clicked() {
                self.checked.extend(shown.iter().map(|c| c.0.clone()));
            }
            if ui.small_button(s.uncheck_all).clicked() {
                for c in &shown {
                    self.checked.remove(&c.0);
                }
            }
        });
        ui.separator();

        let list_height = (ui.available_height() - CARD_HEIGHT).max(80.0);
        egui::ScrollArea::vertical().max_height(list_height).auto_shrink([false, true]).show(ui, |ui| {
            if self.listing.catalogs.is_empty() {
                ui.weak(s.no_catalogs);
            }
            for (file, name, archive, preserved) in &shown {
                self.catalog_row(ui, file, name, *archive, *preserved);
            }
            for (file, reason) in &self.listing.invalid {
                let label = file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                ui.add_enabled(false, egui::Label::new(format!("⚠ {label}")))
                    .on_disabled_hover_text(format!("{}: {reason}", s.invalid_file));
            }
        });
        ui.separator();
        self.details_card(ui);
    }

    fn catalog_row(&mut self, ui: &mut egui::Ui, file: &PathBuf, name: &str, archive: bool, preserved: bool) {
        let s = self.fmt.s;
        ui.horizontal(|ui| {
            let mut checked = self.checked.contains(file);
            let response = ui.checkbox(&mut checked, "");
            response.widget_info(|| {
                egui::WidgetInfo::selected(egui::accesskit::Role::CheckBox, true, checked, format!("☑ {name}"))
            });
            if response.changed() {
                if checked {
                    self.checked.insert(file.clone());
                } else {
                    self.checked.remove(file);
                }
            }
            let selected = self.selected.as_ref() == Some(file);
            let response = ui.selectable_label(selected, name);
            if response.clicked() {
                self.selected = Some(file.clone());
            }
            if response.double_clicked() {
                self.open_folder(file, None, None);
            }
            response.context_menu(|ui| {
                if ui.button(s.browse).clicked() {
                    self.open_folder(file, None, None);
                    ui.close();
                }
                if ui.add_enabled(self.job.is_none() && !preserved, egui::Button::new(s.refresh)).clicked() {
                    self.refresh_catalog(file);
                    ui.close();
                }
                if ui.button(s.rename).clicked() {
                    self.pending = Some(Pending::Rename { file: file.clone(), draft: name.to_owned() });
                    ui.close();
                }
                if ui.button(s.delete).clicked() {
                    self.pending = Some(Pending::Delete { file: file.clone(), name: name.to_owned() });
                    ui.close();
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if preserved {
                    ui.label("🔒").on_hover_text(s.preserved);
                }
                if archive {
                    ui.small(s.badge_archive);
                }
            });
        });
    }

    fn details_card(&mut self, ui: &mut egui::Ui) {
        let s = self.fmt.s;
        let Some(info) = self.selected.as_ref().and_then(|f| self.catalog(f)).cloned() else {
            return;
        };
        let v = &info.volume;
        ui.strong(&v.name);
        if self.comment_draft.as_ref().is_none_or(|(f, _)| *f != info.file) {
            self.comment_draft = Some((info.file.clone(), v.comment.clone()));
        }
        let mut meta = MetaUpdate { comment: v.comment.clone(), archive: v.archive, preserved: v.preserved };
        let mut save = false;
        if let Some((_, draft)) = self.comment_draft.as_mut() {
            let response =
                ui.add(egui::TextEdit::singleline(draft).hint_text(s.comment_hint).desired_width(f32::INFINITY));
            if response.lost_focus() && *draft != v.comment {
                meta.comment.clone_from(draft);
                save = true;
            }
        }
        ui.horizontal(|ui| {
            save |= ui.checkbox(&mut meta.archive, s.archive).changed();
            save |= ui.checkbox(&mut meta.preserved, s.preserved).changed();
        });
        if save {
            if let Err(e) = self.dir.update_meta(&info.file, &meta) {
                self.banner = Some(fill(s.err_other, &[("error", &format!("{e:#}"))]));
            }
            self.reload();
        }
        if let (Some(total), Some(free)) = (v.total_bytes, v.free_bytes) {
            let used = total.saturating_sub(free);
            #[allow(clippy::cast_precision_loss, reason = "a progress bar needs no precision")]
            let fraction = if total == 0 { 0.0 } else { used as f32 / total as f32 };
            ui.add(egui::ProgressBar::new(fraction).desired_height(6.0));
        }
        egui::Grid::new("details").num_columns(2).spacing([8.0, 2.0]).show(ui, |ui| {
            let mut row = |label: &str, value: String| {
                ui.weak(label);
                ui.label(value);
                ui.end_row();
            };
            if let (Some(total), Some(free)) = (v.total_bytes, v.free_bytes) {
                row(s.used, format!("{} / {}", self.fmt.size(total.saturating_sub(free)), self.fmt.size(total)));
                row(s.free, self.fmt.size(free));
            }
            row(s.files, self.fmt.int(v.file_count));
            row(s.folders, self.fmt.int(v.dir_count));
            if v.skipped_count > 0 {
                row(s.skipped, self.fmt.int(v.skipped_count));
            }
            row(s.serial, v.serial.clone().unwrap_or_default());
            row(s.source, v.source_path.clone());
            row(s.cataloged_on, self.fmt.date(Some(v.cataloged_at)));
        });
    }

    /// Re-scans a catalog's medium, found by serial number wherever it is mounted now.
    fn refresh_catalog(&mut self, file: &Path) {
        let Some(info) = self.catalog(file).cloned() else {
            return;
        };
        match self.locate(&info.volume) {
            Some(source) => {
                let opts = ScanOptions {
                    name: info.volume.name.clone(),
                    ignore: IgnoreRules::parse(&self.settings.default_ignore),
                    ..ScanOptions::default()
                };
                self.start_scan(&source, opts);
            }
            None => self.banner = Some(fill(self.fmt.s.medium_not_found, &[("name", &info.volume.name)])),
        }
    }

    /// Shows a folder of a catalog in the Folders tab, optionally selecting one entry.
    pub(super) fn open_folder(&mut self, file: &Path, dir: Option<i64>, select: Option<i64>) {
        let Some(info) = self.catalog(file).cloned() else {
            return;
        };
        let mut folders = std::mem::take(&mut self.folders);
        if let Err(e) = folders.load(info, dir, select) {
            self.banner = Some(fill(self.fmt.s.err_other, &[("error", &format!("{e:#}"))]));
        }
        self.folders = folders;
        self.tab = super::app::Tab::Folders;
    }
}

impl Default for FoldersTab {
    fn default() -> Self {
        FoldersTab::empty()
    }
}
