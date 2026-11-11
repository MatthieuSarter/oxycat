//! Scanning a source folder into a catalog file, atomically.
//!
//! The scan is written to `<stem>.db.tmp` and renamed over the catalog only on success: a failed or cancelled scan
//! leaves the previous catalog untouched. Nothing is ever written to the scanned source.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, params};
use walkdir::WalkDir;

use super::schema;
use super::store::{CatalogDir, CatalogInfo, ReplaceError};
use crate::core::fold::fold;
use crate::core::ignore::IgnoreRules;
use crate::core::locate::source_rel;
use crate::core::model::Volume;
use crate::core::refresh::{DecideError, WriteDecision, decide};
use crate::platform;

/// What the user asked for.
#[derive(Debug, Clone, Default)]
pub struct ScanOptions {
    /// Catalog name.
    pub name: String,
    /// Comment; `None` keeps the existing one on refresh, empty otherwise.
    pub comment: Option<String>,
    /// Archive flag; `None` keeps the existing one on refresh, else follows removable media.
    pub archive: Option<bool>,
    /// Ignore patterns (Cathy syntax); `None` keeps the existing ones on refresh, none otherwise. They are stored
    /// in the catalog.
    pub ignore: Option<String>,
    /// Allow replacing a catalog of another medium or source with the same name.
    pub overwrite: bool,
}

/// Progress and cancellation, shared with the thread showing the progress.
#[derive(Debug, Default)]
pub struct ScanControl {
    /// Entries cataloged so far (updated every 1 000 entries).
    pub entries: AtomicU64,
    /// Unreadable entries so far.
    pub skipped: AtomicU64,
    /// Path being scanned.
    pub current: Mutex<String>,
    /// Set to stop the scan; the previous catalog is kept.
    pub cancel: AtomicBool,
}

/// Why a scan produced no catalog. The previous catalog, if any, is unchanged in every case.
#[derive(Debug)]
pub enum ScanError {
    /// The source folder does not exist or is not a folder.
    SourceMissing,
    /// The name cannot be used (preserved catalog, or another medium without `overwrite`).
    Decide(DecideError),
    /// Cancelled through [`ScanControl::cancel`].
    Cancelled,
    /// The catalog file is locked by another program.
    InUse,
    /// Any other error.
    Other(anyhow::Error),
}

/// Scans `source` into the catalog named `opts.name` in `dir`.
///
/// # Errors
/// See [`ScanError`].
pub fn catalog(
    dir: &CatalogDir,
    source: &Path,
    opts: &ScanOptions,
    ctl: &ScanControl,
) -> Result<CatalogInfo, ScanError> {
    if !source.is_dir() {
        return Err(ScanError::SourceMissing);
    }
    let source = std::path::absolute(source).map_err(|e| ScanError::Other(e.into()))?;
    let info = platform::volume_info(&source);
    let source_path = source.to_string_lossy().into_owned();
    let volume_root = info.root.as_ref().map(|r| r.to_string_lossy().into_owned());
    let existing = dir.find(&opts.name);
    let existing_file = existing.as_ref().map(|c| c.file.clone());
    let existing = existing.map(|c| c.volume);
    let rel = source_rel(&source_path, volume_root.as_deref());
    let decision =
        decide(existing.as_ref(), info.serial.as_deref(), &rel, opts.overwrite).map_err(ScanError::Decide)?;

    let kept = existing.filter(|_| decision == WriteDecision::Refresh);
    let ignore = opts.ignore.clone().or_else(|| kept.as_ref().map(|k| k.ignore.clone())).unwrap_or_default();
    let tmp = dir.temp_path_for(&opts.name);
    let built = build(&tmp, &source, &IgnoreRules::parse(&ignore), ctl);
    let counts = match built {
        Ok(counts) => counts,
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            return Err(e);
        }
    };
    let volume = Volume {
        name: kept.as_ref().map_or_else(|| opts.name.clone(), |k| k.name.clone()),
        comment: opts.comment.clone().or_else(|| kept.as_ref().map(|k| k.comment.clone())).unwrap_or_default(),
        archive: opts.archive.or_else(|| kept.as_ref().map(|k| k.archive)).unwrap_or(info.removable),
        preserved: false,
        serial: info.serial,
        label: info.label,
        filesystem: info.filesystem,
        source_path,
        volume_root,
        total_bytes: info.total_bytes,
        free_bytes: info.free_bytes,
        file_count: counts.files,
        dir_count: counts.dirs,
        skipped_count: counts.skipped,
        cataloged_at: unix_seconds(SystemTime::now()).unwrap_or_default(),
        oxycat_version: env!("CARGO_PKG_VERSION").to_owned(),
        ignore,
    };
    let finish = Connection::open(&tmp).and_then(|conn| schema::write_volume(&conn, &volume));
    if let Err(e) = finish {
        let _ = std::fs::remove_file(&tmp);
        return Err(ScanError::Other(e.into()));
    }

    // Always replace the existing file, whatever the casing of the new name (case-sensitive file systems).
    let target = existing_file.unwrap_or_else(|| dir.path_for(&volume.name));
    dir.replace(&tmp, &target).map_err(|e| match e {
        ReplaceError::InUse => ScanError::InUse,
        ReplaceError::Io(e) => ScanError::Other(e.into()),
    })?;
    let modified = std::fs::metadata(&target).and_then(|m| m.modified()).map_err(|e| ScanError::Other(e.into()))?;
    Ok(CatalogInfo { file: target, volume, modified })
}

/// Entry counts of a finished scan.
struct Counts {
    files: u64,
    dirs: u64,
    skipped: u64,
}

/// Walks `source` and writes its entries into a fresh database at `tmp`.
fn build(tmp: &Path, source: &Path, ignore: &IgnoreRules, ctl: &ScanControl) -> Result<Counts, ScanError> {
    let other = |e: rusqlite::Error| ScanError::Other(e.into());
    if tmp.exists() {
        std::fs::remove_file(tmp).map_err(|e| ScanError::Other(e.into()))?;
    }
    let mut conn = Connection::open(tmp).map_err(other)?;
    schema::create(&conn).map_err(other)?;
    // The file only becomes a catalog through an atomic rename, so durability while building is useless.
    conn.pragma_update(None, "journal_mode", "OFF").map_err(other)?;
    conn.pragma_update(None, "synchronous", "OFF").map_err(other)?;
    let tx = conn.transaction().map_err(other)?;
    let mut counts = Counts { files: 0, dirs: 0, skipped: 0 };
    {
        let mut insert = tx
            .prepare(
                "INSERT INTO entries (parent_id, name, name_fold, is_dir, size, mtime) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )
            .map_err(other)?;
        let mut dir_ids: HashMap<PathBuf, i64> = HashMap::new();
        let walker =
            WalkDir::new(source).follow_links(false).min_depth(1).into_iter().filter_entry(|e| {
                !(e.file_type().is_dir() && ignore.is_ignored(&e.file_name().to_string_lossy(), true))
            });
        for item in walker {
            if ctl.cancel.load(Ordering::Relaxed) {
                return Err(ScanError::Cancelled);
            }
            let Ok(entry) = item else {
                counts.skipped += 1;
                ctl.skipped.store(counts.skipped, Ordering::Relaxed);
                continue;
            };
            let is_dir = entry.file_type().is_dir();
            let name = entry.file_name().to_string_lossy().into_owned();
            if !is_dir && ignore.is_ignored(&name, false) {
                continue;
            }
            let parent_id = match entry.path().parent() {
                Some(parent) if entry.depth() > 1 => match dir_ids.get(parent) {
                    Some(id) => Some(*id),
                    None => continue,
                },
                _ => None,
            };
            let meta = entry.metadata().ok();
            let size = if is_dir { None } else { meta.as_ref().map(std::fs::Metadata::len) };
            let mtime = meta.and_then(|m| m.modified().ok()).and_then(unix_seconds);
            insert.execute(params![parent_id, name, fold(&name), is_dir, size, mtime]).map_err(other)?;
            if is_dir {
                dir_ids.insert(entry.path().to_path_buf(), tx.last_insert_rowid());
                counts.dirs += 1;
            } else {
                counts.files += 1;
            }
            let total = counts.files + counts.dirs;
            if total.is_multiple_of(1000) {
                ctl.entries.store(total, Ordering::Relaxed);
                if let Ok(mut current) = ctl.current.lock() {
                    *current = entry.path().to_string_lossy().into_owned();
                }
            }
        }
    }
    tx.commit().map_err(other)?;
    conn.pragma_update(None, "journal_mode", "DELETE").map_err(other)?;
    ctl.entries.store(counts.files + counts.dirs, Ordering::Relaxed);
    Ok(counts)
}

/// Seconds since the Unix epoch (negative before it), `None` when out of range.
fn unix_seconds(time: SystemTime) -> Option<i64> {
    match time.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_secs()).ok(),
        Err(e) => i64::try_from(e.duration().as_secs()).ok().map(|s| -s),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::atomic::Ordering;

    use rusqlite::Connection;

    use super::{ScanControl, ScanError, ScanOptions, catalog};
    use crate::catalog::store::{CatalogDir, MetaUpdate, open_read};
    use crate::core::refresh::DecideError;

    struct Fixture {
        _src_tmp: tempfile::TempDir,
        _cat_tmp: tempfile::TempDir,
        src: std::path::PathBuf,
        dir: CatalogDir,
    }

    fn fixture() -> Fixture {
        let src_tmp = tempfile::tempdir().unwrap();
        let cat_tmp = tempfile::tempdir().unwrap();
        let src = src_tmp.path().join("src");
        std::fs::create_dir_all(src.join("BD").join("Astérix")).unwrap();
        std::fs::write(src.join("BD").join("Astérix").join("01.cbz"), b"abc").unwrap();
        std::fs::write(src.join("BD").join("x.tmp"), b"").unwrap();
        std::fs::create_dir_all(src.join("Temp")).unwrap();
        std::fs::write(src.join("Temp").join("a.txt"), b"a").unwrap();
        Fixture { dir: CatalogDir::new(cat_tmp.path().to_path_buf()), src, _src_tmp: src_tmp, _cat_tmp: cat_tmp }
    }

    fn opts(name: &str) -> ScanOptions {
        ScanOptions { name: name.to_owned(), ..ScanOptions::default() }
    }

    fn names(file: &Path) -> Vec<(String, bool, Option<u64>)> {
        let conn = open_read(file).unwrap();
        let mut stmt = conn.prepare("SELECT name_fold, is_dir, size FROM entries ORDER BY name_fold").unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).unwrap().map(Result::unwrap).collect()
    }

    #[test]
    fn scan_builds_tree() {
        let f = fixture();
        let info = catalog(&f.dir, &f.src, &opts("T"), &ScanControl::default()).unwrap();
        assert_eq!(info.volume.name, "T");
        assert_eq!((info.volume.file_count, info.volume.dir_count), (3, 3));
        let conn = open_read(&info.file).unwrap();
        let (dir_name, parent): (String, String) = conn
            .query_row(
                "SELECT e.name, p.name FROM entries e JOIN entries p ON p.id = e.parent_id WHERE e.name_fold = 'asterix'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((dir_name.as_str(), parent.as_str()), ("Astérix", "BD"));
        assert!(names(&info.file).contains(&("01.cbz".to_owned(), false, Some(3))));
        let roots: i64 =
            conn.query_row("SELECT count(*) FROM entries WHERE parent_id IS NULL", [], |r| r.get(0)).unwrap();
        assert_eq!(roots, 2);
        assert!(info.volume.total_bytes.is_some());
    }

    #[test]
    fn ignore_rules_applied() {
        let f = fixture();
        let o = ScanOptions { ignore: Some(r"*.tmp;\Temp*".to_owned()), ..opts("T") };
        let info = catalog(&f.dir, &f.src, &o, &ScanControl::default()).unwrap();
        let all = names(&info.file);
        assert!(all.iter().all(|(n, _, _)| n != "x.tmp" && n != "temp" && n != "a.txt"), "{all:?}");
        assert_eq!(info.volume.ignore, r"*.tmp;\Temp*");
    }

    #[test]
    fn refresh_reuses_stored_ignore_patterns() {
        let f = fixture();
        let o = ScanOptions { ignore: Some("*.tmp".to_owned()), ..opts("T") };
        let first = catalog(&f.dir, &f.src, &o, &ScanControl::default()).unwrap();
        let refreshed = catalog(&f.dir, &f.src, &opts("T"), &ScanControl::default()).unwrap();
        assert_eq!(refreshed.volume.ignore, "*.tmp");
        assert_eq!(refreshed.volume.file_count, first.volume.file_count);
        assert!(names(&refreshed.file).iter().all(|(n, _, _)| n != "x.tmp"));
    }

    #[test]
    fn rescan_same_source_is_refresh_and_keeps_comment() {
        let f = fixture();
        let first = catalog(&f.dir, &f.src, &opts("T"), &ScanControl::default()).unwrap();
        let meta = MetaUpdate { comment: "BD".to_owned(), archive: true, preserved: false, ignore: String::new() };
        f.dir.update_meta(&first.file, &meta).unwrap();
        std::fs::write(f.src.join("new.txt"), b"n").unwrap();
        let second = catalog(&f.dir, &f.src, &opts("t"), &ScanControl::default()).unwrap();
        assert_eq!(second.volume.name, "T");
        assert_eq!(second.volume.comment, "BD");
        assert!(second.volume.archive);
        assert_eq!(second.volume.file_count, 4);
    }

    #[test]
    fn other_source_same_name_needs_overwrite() {
        let f = fixture();
        catalog(&f.dir, &f.src, &opts("T"), &ScanControl::default()).unwrap();
        let other = f.src.join("BD");
        let refused = catalog(&f.dir, &other, &opts("T"), &ScanControl::default());
        assert!(matches!(refused, Err(ScanError::Decide(DecideError::NameTaken))), "{refused:?}");
        let o = ScanOptions { overwrite: true, comment: Some("c".to_owned()), ..opts("T") };
        let replaced = catalog(&f.dir, &other, &o, &ScanControl::default()).unwrap();
        assert_eq!((replaced.volume.file_count, replaced.volume.comment.as_str()), (2, "c"));
    }

    #[test]
    fn preserved_refused_nothing_written() {
        let f = fixture();
        let first = catalog(&f.dir, &f.src, &opts("T"), &ScanControl::default()).unwrap();
        let meta = MetaUpdate { comment: String::new(), archive: false, preserved: true, ignore: String::new() };
        f.dir.update_meta(&first.file, &meta).unwrap();
        let before = std::fs::read(&first.file).unwrap();
        let result = catalog(&f.dir, &f.src, &ScanOptions { overwrite: true, ..opts("T") }, &ScanControl::default());
        assert!(matches!(result, Err(ScanError::Decide(DecideError::Preserved))), "{result:?}");
        assert_eq!(std::fs::read(&first.file).unwrap(), before);
    }

    #[test]
    fn missing_source_errors_nothing_written() {
        let f = fixture();
        let result = catalog(&f.dir, &f.src.join("nope"), &opts("T"), &ScanControl::default());
        assert!(matches!(result, Err(ScanError::SourceMissing)), "{result:?}");
        assert_eq!(std::fs::read_dir(f.dir.root()).unwrap().count(), 0);
    }

    #[test]
    fn cancel_keeps_old_catalog() {
        let f = fixture();
        let first = catalog(&f.dir, &f.src, &opts("T"), &ScanControl::default()).unwrap();
        let before = std::fs::read(&first.file).unwrap();
        let ctl = ScanControl::default();
        ctl.cancel.store(true, Ordering::Relaxed);
        let result = catalog(&f.dir, &f.src, &opts("T"), &ctl);
        assert!(matches!(result, Err(ScanError::Cancelled)), "{result:?}");
        assert_eq!(std::fs::read(&first.file).unwrap(), before);
        assert!(!f.dir.temp_path_for("T").exists());
    }

    #[test]
    fn progress_reported() {
        let f = fixture();
        let ctl = ScanControl::default();
        catalog(&f.dir, &f.src, &opts("T"), &ctl).unwrap();
        assert_eq!(ctl.entries.load(Ordering::Relaxed), 6);
    }

    #[test]
    fn long_path_scanned() {
        let f = fixture();
        let mut deep = f.src.clone();
        for i in 0..30 {
            deep.push(format!("folder-{i:04}"));
        }
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("deep.txt"), b"d").unwrap();
        assert!(deep.as_os_str().len() > 300);
        let info = catalog(&f.dir, &f.src, &opts("T"), &ScanControl::default()).unwrap();
        assert!(names(&info.file).iter().any(|(n, _, _)| n == "deep.txt"));
    }

    #[test]
    fn refresh_with_other_case_reuses_the_existing_file() {
        let f = fixture();
        let first = catalog(&f.dir, &f.src, &opts("b1"), &ScanControl::default()).unwrap();
        let second = catalog(&f.dir, &f.src, &opts("B1"), &ScanControl::default()).unwrap();
        assert_eq!(second.file, first.file);
        // Overwriting with another source under another casing must replace the same file, not add a twin
        // (case-sensitive file systems would otherwise keep both b1.db and B1.db).
        let o = ScanOptions { overwrite: true, ..opts("B1") };
        let third = catalog(&f.dir, &f.src.join("BD"), &o, &ScanControl::default()).unwrap();
        assert_eq!(third.file, first.file);
        assert_eq!(f.dir.list().catalogs.len(), 1);
    }

    #[test]
    fn ignore_rules_do_not_apply_to_the_source_itself() {
        let f = fixture();
        let o = ScanOptions { ignore: Some(r"\Temp*".to_owned()), ..opts("T") };
        let info = catalog(&f.dir, &f.src.join("Temp"), &o, &ScanControl::default()).unwrap();
        assert_eq!(info.volume.file_count, 1);
    }

    #[test]
    fn leftover_temp_file_replaced() {
        let f = fixture();
        std::fs::write(f.dir.temp_path_for("T"), b"garbage").unwrap();
        catalog(&f.dir, &f.src, &opts("T"), &ScanControl::default()).unwrap();
        assert!(!f.dir.temp_path_for("T").exists());
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_dir_skipped() {
        use std::os::unix::fs::PermissionsExt;
        let f = fixture();
        let locked = f.src.join("locked");
        std::fs::create_dir(&locked).unwrap();
        std::fs::write(locked.join("hidden.txt"), b"h").unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        let result = catalog(&f.dir, &f.src, &opts("T"), &ScanControl::default());
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        // Root ignores permissions; only check when the folder was really unreadable.
        if std::fs::read_dir(&locked).is_ok() && result.as_ref().unwrap().volume.skipped_count == 0 {
            return;
        }
        assert_eq!(result.unwrap().volume.skipped_count, 1);
    }

    #[cfg(unix)]
    #[test]
    fn symlink_not_followed() {
        let f = fixture();
        std::os::unix::fs::symlink(f.src.join("BD"), f.src.join("link")).unwrap();
        let info = catalog(&f.dir, &f.src, &opts("T"), &ScanControl::default()).unwrap();
        let all = names(&info.file);
        assert!(all.contains(&("link".to_owned(), false, Some(0))) || all.iter().any(|(n, d, _)| n == "link" && !d));
        assert_eq!(all.iter().filter(|(n, _, _)| n == "01.cbz").count(), 1);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn non_utf8_name_lossy() {
        use std::os::unix::ffi::OsStrExt;
        let f = fixture();
        std::fs::write(f.src.join(std::ffi::OsStr::from_bytes(b"bad\xffname")), b"x").unwrap();
        let info = catalog(&f.dir, &f.src, &opts("T"), &ScanControl::default()).unwrap();
        assert!(names(&info.file).iter().any(|(n, _, _)| n.starts_with("bad") && n.ends_with("name")));
    }

    #[test]
    fn temp_db_is_valid_sqlite_after_scan() {
        let f = fixture();
        let info = catalog(&f.dir, &f.src, &opts("T"), &ScanControl::default()).unwrap();
        let mode: String =
            Connection::open(&info.file).unwrap().pragma_query_value(None, "journal_mode", |r| r.get(0)).unwrap();
        assert_eq!(mode, "delete");
    }
}
