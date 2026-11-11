//! Cataloging from the GUI: the add dialog, the background scan job with its progress, and the confirmations.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use super::app::{OxycatApp, Pending};
use crate::catalog::scan::{ScanControl, ScanError, ScanOptions, catalog};
use crate::catalog::store::CatalogInfo;
use crate::cli::scan_error_message;
use crate::config::Settings;
use crate::core::locate::source_rel;
use crate::core::refresh::{DecideError, decide};
use crate::i18n::fill;
use crate::platform;

/// The "catalog a medium" form.
#[derive(Debug, Clone)]
pub(super) struct AddDialog {
    source: String,
    name: String,
    comment: String,
    archive: bool,
    ignore: String,
}

impl AddDialog {
    pub fn empty(settings: &Settings) -> AddDialog {
        AddDialog {
            source: String::new(),
            name: String::new(),
            comment: String::new(),
            archive: false,
            ignore: settings.default_ignore.clone(),
        }
    }

    pub fn for_source(path: &Path, settings: &Settings) -> AddDialog {
        let mut dialog = AddDialog::empty(settings);
        dialog.set_source(path);
        dialog
    }

    /// Scan options from the form.
    fn options(&self, overwrite: bool) -> ScanOptions {
        ScanOptions {
            name: self.name.trim().to_owned(),
            comment: Some(self.comment.clone()),
            archive: Some(self.archive),
            ignore: Some(self.ignore.trim().to_owned()),
            overwrite,
        }
    }

    /// Fills the source and proposes a name (volume label, else folder name, else drive) and the archive flag.
    fn set_source(&mut self, path: &Path) {
        self.source = path.to_string_lossy().into_owned();
        let info = platform::volume_info(path);
        self.archive = info.removable;
        let is_root = info.root.as_deref().is_some_and(|r| std::path::absolute(path).is_ok_and(|p| p == r));
        let folder = path.file_name().map(|n| n.to_string_lossy().into_owned());
        let drive = self.source.trim_end_matches(['\\', '/', ':']).to_owned();
        self.name = if is_root { info.label.or(folder) } else { folder.or(info.label) }.unwrap_or(drive);
    }
}

/// A scan running on a background thread.
pub(super) struct ScanJob {
    name: String,
    source: String,
    ctl: Arc<ScanControl>,
    rx: Receiver<Result<CatalogInfo, ScanError>>,
}

impl OxycatApp {
    /// Starts a scan on a background thread.
    pub(super) fn start_scan(&mut self, source: &Path, opts: ScanOptions) {
        let name = opts.name.clone();
        let ctl = Arc::new(ScanControl::default());
        let (tx, rx) = channel();
        let (dir, source_path, thread_ctl, ctx) =
            (self.dir.clone(), source.to_path_buf(), Arc::clone(&ctl), self.ctx.clone());
        let spawned = std::thread::Builder::new().name("oxycat-scan".to_owned()).spawn(move || {
            let result = catalog(&dir, &source_path, &opts, &thread_ctl);
            let _ = tx.send(result);
            ctx.request_repaint();
        });
        match spawned {
            Ok(_) => {
                let source = source.to_string_lossy().into_owned();
                self.job = Some(ScanJob { name, source, ctl, rx });
            }
            Err(e) => self.banner = Some(fill(self.fmt.s.err_other, &[("error", &e.to_string())])),
        }
    }

    /// Collects the result of a finished scan.
    pub(super) fn poll_job(&mut self) {
        let Some(job) = &self.job else {
            return;
        };
        let result = match job.rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return,
            Err(TryRecvError::Disconnected) => Err(ScanError::Other(anyhow::anyhow!("scan thread stopped"))),
        };
        let (name, source) = (job.name.clone(), job.source.clone());
        self.job = None;
        let s = self.fmt.s;
        self.banner = Some(match result {
            Ok(info) => {
                self.reload();
                self.checked.insert(info.file.clone());
                self.selected = Some(info.file.clone());
                self.card_draft = None;
                let (files, dirs) = (self.fmt.int(info.volume.file_count), self.fmt.int(info.volume.dir_count));
                fill(s.cli_created, &[("name", &info.volume.name), ("files", &files), ("dirs", &dirs)])
            }
            Err(e) => scan_error_message(&e, &name, &source, self.lang),
        });
    }

    /// The add dialog, confirmations and scan progress.
    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        self.add_dialog(ctx);
        self.pending_dialog(ctx);
        self.progress_dialog(ctx);
    }

    fn add_dialog(&mut self, ctx: &egui::Context) {
        let s = self.fmt.s;
        let Some(mut dialog) = self.add.take() else {
            return;
        };
        let mut keep = true;
        let mut start = false;
        let modal = egui::Modal::new(egui::Id::unique("add")).show(ctx, |ui| {
            ui.heading(s.add_title);
            egui::Grid::new("add_grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label(s.source);
                ui.horizontal(|ui| {
                    let response = ui.add(egui::TextEdit::singleline(&mut dialog.source).desired_width(260.0));
                    if response.lost_focus() && Path::new(&dialog.source).is_dir() && dialog.name.is_empty() {
                        let source = PathBuf::from(&dialog.source);
                        dialog.set_source(&source);
                    }
                    if ui.button(s.choose_folder).clicked()
                        && let Some(folder) = rfd::FileDialog::new().pick_folder()
                    {
                        dialog.set_source(&folder);
                    }
                });
                ui.end_row();
                ui.label(s.name);
                ui.text_edit_singleline(&mut dialog.name);
                ui.end_row();
                ui.label(s.comment);
                ui.text_edit_singleline(&mut dialog.comment);
                ui.end_row();
                ui.label(s.ignore);
                ui.add(egui::TextEdit::singleline(&mut dialog.ignore).hint_text(s.ignore_hint));
                ui.end_row();
                ui.label("");
                ui.checkbox(&mut dialog.archive, s.archive);
                ui.end_row();
            });
            ui.separator();
            ui.horizontal(|ui| {
                let ready = !dialog.source.trim().is_empty() && !dialog.name.trim().is_empty();
                if ui.add_enabled(ready, egui::Button::new(s.start)).clicked() {
                    start = true;
                }
                if ui.button(s.cancel).clicked() {
                    keep = false;
                }
            });
        });
        if modal.should_close() {
            keep = false;
        }
        if start {
            self.submit_add(&dialog);
        } else if keep {
            self.add = Some(dialog);
        }
    }

    /// Checks the name against existing catalogs, then starts the scan or asks before overwriting.
    fn submit_add(&mut self, dialog: &AddDialog) {
        let s = self.fmt.s;
        let source = PathBuf::from(dialog.source.trim());
        let name = dialog.name.trim().to_owned();
        if !source.is_dir() {
            self.banner = Some(fill(s.err_source_missing, &[("path", &dialog.source)]));
            return;
        }
        let info = platform::volume_info(&source);
        let absolute = std::path::absolute(&source).unwrap_or_else(|_| source.clone());
        let root = info.root.as_ref().map(|r| r.to_string_lossy().into_owned());
        let rel = source_rel(&absolute.to_string_lossy(), root.as_deref());
        let existing = self.dir.find(&name).map(|c| c.volume);
        match decide(existing.as_ref(), info.serial.as_deref(), &rel, false) {
            Err(DecideError::NameTaken) => self.pending = Some(Pending::Overwrite(dialog.clone())),
            Err(DecideError::Preserved) => self.banner = Some(fill(s.err_preserved, &[("name", &name)])),
            Ok(_) => self.start_scan(&source, dialog.options(false)),
        }
    }

    fn pending_dialog(&mut self, ctx: &egui::Context) {
        let s = self.fmt.s;
        let Some(mut pending) = self.pending.take() else {
            return;
        };
        let mut answer: Option<bool> = None;
        let modal = egui::Modal::new(egui::Id::unique("confirm")).show(ctx, |ui| {
            match &mut pending {
                Pending::Delete { name, .. } => {
                    ui.label(fill(s.confirm_delete, &[("name", name)]));
                }
                Pending::Rename { draft, .. } => {
                    ui.label(s.new_name);
                    let response = ui.text_edit_singleline(draft);
                    if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        answer = Some(true);
                    }
                }
                Pending::Overwrite(dialog) => {
                    ui.label(fill(s.confirm_overwrite, &[("name", dialog.name.trim())]));
                }
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button(s.ok).clicked() {
                    answer = Some(true);
                }
                if ui.button(s.cancel).clicked() {
                    answer = Some(false);
                }
            });
        });
        if modal.should_close() {
            answer = Some(false);
        }
        match answer {
            None => self.pending = Some(pending),
            Some(false) => {}
            Some(true) => self.confirm(pending),
        }
    }

    fn confirm(&mut self, pending: Pending) {
        let s = self.fmt.s;
        let result = match pending {
            Pending::Delete { file, .. } => self.dir.delete(&file).map(|()| None),
            Pending::Rename { file, draft } => self.dir.rename(&file, draft.trim()).map(Some),
            Pending::Overwrite(dialog) => {
                self.start_scan(&PathBuf::from(dialog.source.trim()), dialog.options(true));
                Ok(None)
            }
        };
        match result {
            Ok(new_file) => {
                if new_file.is_some() {
                    self.selected = new_file;
                }
                self.reload();
            }
            Err(e) => self.banner = Some(fill(s.err_other, &[("error", &format!("{e:#}"))])),
        }
    }

    fn progress_dialog(&mut self, ctx: &egui::Context) {
        let s = self.fmt.s;
        let Some(job) = &self.job else {
            return;
        };
        let entries = self.fmt.int(job.ctl.entries.load(Ordering::Relaxed));
        let skipped = self.fmt.int(job.ctl.skipped.load(Ordering::Relaxed));
        let current = job.ctl.current.lock().map(|c| c.clone()).unwrap_or_default();
        let mut cancel = false;
        egui::Modal::new(egui::Id::unique("progress")).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.heading(fill(s.scanning, &[("name", &job.name)]));
            });
            ui.label(fill(s.entries_found, &[("n", &entries)]));
            ui.label(fill(s.entries_skipped, &[("n", &skipped)]));
            ui.add(egui::Label::new(egui::RichText::new(current).weak()).truncate());
            ui.separator();
            cancel = ui.button(s.cancel).clicked();
        });
        if cancel {
            job.ctl.cancel.store(true, Ordering::Relaxed);
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use std::sync::mpsc::channel;

    use egui_kittest::kittest::Queryable;

    use super::{AddDialog, ScanJob};
    use crate::catalog::query::tests::catalog_of;
    use crate::catalog::scan::ScanControl;
    use crate::catalog::store::CatalogDir;
    use crate::config::Settings;
    use crate::gui::app::Pending;
    use crate::gui::tests::{harness, shows, step_until};

    /// Lets the add dialog finish its layout: until then its buttons are not where the accessibility tree says.
    fn settle(harness: &mut egui_kittest::Harness<'static, crate::gui::app::OxycatApp>) {
        harness.run_steps(8);
    }

    #[test]
    fn add_dialog_catalogs_a_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("Photos");
        std::fs::create_dir_all(src.join("2026")).unwrap();
        std::fs::write(src.join("2026").join("a.jpg"), b"jpg").unwrap();
        let mut harness = harness(CatalogDir::new(tmp.path().join("catalogs")));
        harness.run_steps(2);
        harness.state_mut().add = Some(AddDialog::for_source(&src, &Settings::default()));
        settle(&mut harness);
        // The name is proposed from the folder.
        assert!(shows(&harness, "Photos"));
        harness.get_by_label("Catalog").click();
        step_until(&mut harness, |app| app.job.is_none() && !app.listing.catalogs.is_empty());
        let banner = harness.state().banner.clone().unwrap_or_default();
        assert!(banner.contains("Catalog \"Photos\" created: 1 files, 1 folders"), "{banner:?}");
        assert!(harness.state().add.is_none());
    }

    #[test]
    fn missing_source_is_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let mut harness = harness(CatalogDir::new(tmp.path().to_path_buf()));
        harness.state_mut().add = Some(AddDialog::for_source(&tmp.path().join("gone"), &Settings::default()));
        settle(&mut harness);
        harness.get_by_label("Catalog").click();
        harness.run_steps(2);
        let banner = harness.state().banner.clone().unwrap_or_default();
        assert!(banner.contains("not found"), "{banner:?}");
        assert!(harness.state().job.is_none());
    }

    #[test]
    fn cancel_closes_the_add_dialog() {
        let tmp = tempfile::tempdir().unwrap();
        let mut harness = harness(CatalogDir::new(tmp.path().to_path_buf()));
        harness.state_mut().add = Some(AddDialog::empty(&Settings::default()));
        harness.run_steps(2);
        settle(&mut harness);
        harness.get_by_label("Cancel").click();
        harness.run_steps(2);
        assert!(harness.state().add.is_none());
    }

    #[test]
    fn same_name_for_another_source_asks_before_overwriting() {
        let (tmp, first) = catalog_of("B1", &[("a.txt", Some(1))]);
        let other = tmp.path().join("other").join("B1");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("b.txt"), b"b").unwrap();
        std::fs::write(other.join("c.txt"), b"c").unwrap();
        let mut harness = harness(CatalogDir::new(tmp.path().to_path_buf()));
        harness.run_steps(2);
        harness.state_mut().add = Some(AddDialog::for_source(&other, &Settings::default()));
        settle(&mut harness);
        harness.get_by_label("Catalog").click();
        harness.run_steps(2);
        assert!(matches!(harness.state().pending, Some(Pending::Overwrite(_))));
        harness.get_by_label("OK").click();
        step_until(&mut harness, |app| {
            app.job.is_none() && app.listing.catalogs.first().is_some_and(|c| c.volume.file_count == 2)
        });
        assert_eq!(harness.state().listing.catalogs.len(), 1);
        assert_eq!(harness.state().listing.catalogs[0].file, first.file);
    }

    #[test]
    fn rename_then_delete_a_catalog() {
        let (tmp, info) = catalog_of("B1", &[("a.txt", Some(1))]);
        let mut harness = harness(CatalogDir::new(tmp.path().to_path_buf()));
        harness.run_steps(2);
        harness.state_mut().pending = Some(Pending::Rename { file: info.file.clone(), draft: "Blue".to_owned() });
        harness.run_steps(2);
        harness.get_by_label("OK").click();
        harness.run_steps(3);
        let renamed = harness.state().listing.catalogs[0].clone();
        assert_eq!(renamed.volume.name, "Blue");
        assert_eq!(harness.state().selected.as_ref(), Some(&renamed.file));

        // Cancel keeps the catalog, OK deletes it.
        let pending = || Pending::Delete { file: renamed.file.clone(), name: "Blue".to_owned() };
        harness.state_mut().pending = Some(pending());
        harness.run_steps(2);
        harness.get_by_label("Cancel").click();
        harness.run_steps(2);
        assert!(renamed.file.exists());
        harness.state_mut().pending = Some(pending());
        harness.run_steps(2);
        harness.get_by_label("OK").click();
        harness.run_steps(3);
        assert!(!renamed.file.exists());
        assert_eq!(harness.state().listing.catalogs, [] as [crate::catalog::store::CatalogInfo; 0]);
    }

    #[test]
    fn failed_rename_is_reported() {
        let (tmp, info) = catalog_of("B1", &[("a.txt", Some(1))]);
        let mut harness = harness(CatalogDir::new(tmp.path().to_path_buf()));
        harness.run_steps(2);
        // The catalog file vanished in the meantime.
        std::fs::remove_file(&info.file).unwrap();
        harness.state_mut().pending = Some(Pending::Rename { file: info.file.clone(), draft: "Blue".to_owned() });
        harness.run_steps(2);
        harness.get_by_label("OK").click();
        harness.run_steps(2);
        let banner = harness.state().banner.clone().unwrap_or_default();
        assert!(banner.starts_with("Error: "), "{banner:?}");
    }

    #[test]
    fn progress_cancel_and_lost_scan_thread() {
        let tmp = tempfile::tempdir().unwrap();
        let mut harness = harness(CatalogDir::new(tmp.path().to_path_buf()));
        let ctl = Arc::new(ScanControl::default());
        ctl.entries.store(1234, Ordering::Relaxed);
        let (tx, rx) = channel();
        let job = ScanJob { name: "B1".to_owned(), source: "E:\\".to_owned(), ctl: Arc::clone(&ctl), rx };
        harness.state_mut().job = Some(job);
        harness.run_steps(2);
        assert!(shows(&harness, "Cataloging \"B1\"…"));
        harness.get_by_label("Cancel").click();
        harness.run_steps(2);
        assert!(ctl.cancel.load(Ordering::Relaxed));

        // The scan thread dies without answering: the job ends with an error instead of spinning forever.
        drop(tx);
        harness.run_steps(2);
        assert!(harness.state().job.is_none());
        let banner = harness.state().banner.clone().unwrap_or_default();
        assert!(banner.contains("scan thread stopped"), "{banner:?}");
    }
}
