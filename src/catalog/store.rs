//! The catalog directory: listing, opening, editing, renaming, deleting and atomically replacing catalog files.
//!
//! Connections are opened per operation and dropped right away: an open handle would block [`CatalogDir::replace`]
//! on Windows.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, bail};
use rusqlite::{Connection, OpenFlags};

use super::schema::{SCHEMA_VERSION, read_volume, upgrade};
use crate::core::model::Volume;
use crate::core::names::catalog_file_stem;

/// A directory holding catalog files (`*.db`).
#[derive(Debug, Clone)]
pub struct CatalogDir {
    root: PathBuf,
}

/// A valid catalog file and its volume description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogInfo {
    /// Path of the `.db` file.
    pub file: PathBuf,
    /// Its volume row.
    pub volume: Volume,
    /// Last modification time of the file.
    pub modified: SystemTime,
}

/// Content of a catalog directory.
#[derive(Debug, Default)]
pub struct Listing {
    /// Valid catalogs, sorted by name (case-insensitive).
    pub catalogs: Vec<CatalogInfo>,
    /// `.db` files that are not readable Oxycat catalogs, with the reason.
    pub invalid: Vec<(PathBuf, String)>,
}

/// Editable flags of a catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaUpdate {
    /// Free-form comment.
    pub comment: String,
    /// Archive flag.
    pub archive: bool,
    /// Preserved flag.
    pub preserved: bool,
    /// Ignore patterns applied at the next refresh.
    pub ignore: String,
}

/// Why [`CatalogDir::replace`] failed. In every case the previous catalog is left untouched.
#[derive(Debug)]
pub enum ReplaceError {
    /// The target file is locked by another program (or by an open connection).
    InUse,
    /// Any other I/O error.
    Io(std::io::Error),
}

/// Opens a catalog read-only and checks it is an Oxycat catalog of the supported schema version.
///
/// # Errors
/// When the file cannot be opened, has another `user_version`, or has no volume row.
pub fn open_read(file: &Path) -> anyhow::Result<Connection> {
    let conn = Connection::open_with_flags(file, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    check_catalog(&conn)?;
    Ok(conn)
}

/// Opens a catalog for writing (never creates one) after the same checks as [`open_read`].
fn open_write(file: &Path) -> anyhow::Result<Connection> {
    let conn = Connection::open_with_flags(file, OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    check_catalog(&conn)?;
    Ok(conn)
}

/// Upgrades an older Oxycat catalog in place. The version is read first through a read-only connection, so that
/// current catalogs (and other SQLite files) are never opened for writing; a failure shows up later as an invalid
/// file.
fn upgrade_if_old(file: &Path) {
    let version = Connection::open_with_flags(file, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .and_then(|conn| conn.pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0)));
    if version.is_ok_and(|v| (1..SCHEMA_VERSION).contains(&v)) {
        let _ = Connection::open_with_flags(file, OpenFlags::SQLITE_OPEN_READ_WRITE).map(|c| upgrade(&c));
    }
}

/// Fails unless the database has the supported schema version and a volume row.
fn check_catalog(conn: &Connection) -> anyhow::Result<Volume> {
    let version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version != SCHEMA_VERSION {
        bail!("unsupported catalog version {version} (expected {SCHEMA_VERSION})");
    }
    read_volume(conn).context("not an Oxycat catalog")
}

/// Case-insensitive file stem of a path.
fn stem_key(file: &Path) -> String {
    file.file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default()
}

impl CatalogDir {
    /// Wraps a directory path.
    #[must_use]
    pub fn new(root: PathBuf) -> CatalogDir {
        CatalogDir { root }
    }

    /// The directory path.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Lists the `.db` files of the directory.
    #[must_use]
    pub fn list(&self) -> Listing {
        let mut listing = Listing::default();
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return listing;
        };
        for path in entries.filter_map(Result::ok).map(|e| e.path()) {
            let is_db = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("db")) && path.is_file();
            if !is_db {
                continue;
            }
            upgrade_if_old(&path);
            let info = open_read(&path).and_then(|conn| {
                let volume = read_volume(&conn)?;
                let modified = std::fs::metadata(&path)?.modified()?;
                Ok(CatalogInfo { file: path.clone(), volume, modified })
            });
            match info {
                Ok(info) => listing.catalogs.push(info),
                Err(e) => listing.invalid.push((path, format!("{e:#}"))),
            }
        }
        listing.catalogs.sort_by_cached_key(|c| c.volume.name.to_lowercase());
        listing
    }

    /// Path of the catalog file for a catalog name.
    #[must_use]
    pub fn path_for(&self, name: &str) -> PathBuf {
        self.root.join(format!("{}.db", catalog_file_stem(name)))
    }

    /// Path of the temporary file a scan of this catalog writes into.
    #[must_use]
    pub fn temp_path_for(&self, name: &str) -> PathBuf {
        self.root.join(format!("{}.db.tmp", catalog_file_stem(name)))
    }

    /// Finds the catalog whose file would be used for `name` (case-insensitive).
    #[must_use]
    pub fn find(&self, name: &str) -> Option<CatalogInfo> {
        let key = stem_key(&self.path_for(name));
        self.list().catalogs.into_iter().find(|c| stem_key(&c.file) == key)
    }

    /// Updates the comment, flags and ignore patterns of a catalog.
    ///
    /// # Errors
    /// When the file cannot be opened for writing or updated.
    pub fn update_meta(&self, file: &Path, meta: &MetaUpdate) -> anyhow::Result<()> {
        let conn = open_write(file)?;
        conn.execute(
            "UPDATE volume SET comment = ?1, archive = ?2, preserved = ?3, ignore_patterns = ?4 WHERE id = 1",
            rusqlite::params![meta.comment, meta.archive, meta.preserved, meta.ignore],
        )?;
        Ok(())
    }

    /// Renames a catalog (its stored name and its file) and returns the new file path.
    ///
    /// # Errors
    /// When another catalog already uses the target file name, or on I/O or SQLite errors.
    pub fn rename(&self, file: &Path, new_name: &str) -> anyhow::Result<PathBuf> {
        let target = self.path_for(new_name);
        // Any other `.db` file with the same name, whatever its casing, is a conflict (case-sensitive file systems
        // can hold both `b1.db` and `B1.db`); a case-only rename of `file` itself is fine.
        let key = stem_key(&target);
        let conflict = std::fs::read_dir(&self.root)?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .any(|p| p != file && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("db")) && stem_key(&p) == key);
        if conflict {
            bail!("a catalog file {} already exists", target.display());
        }
        let old_name = check_catalog(&open_read(file)?)?.name;
        let set_name = |name: &str, path: &Path| -> anyhow::Result<()> {
            open_write(path)?.execute("UPDATE volume SET name = ?1 WHERE id = 1", [name])?;
            Ok(())
        };
        set_name(new_name, file)?;
        if let Err(e) = std::fs::rename(file, &target) {
            set_name(&old_name, file)?;
            return Err(e).context("cannot rename the catalog file");
        }
        Ok(target)
    }

    /// Deletes a catalog file.
    ///
    /// # Errors
    /// Any I/O error.
    pub fn delete(&self, file: &Path) -> anyhow::Result<()> {
        std::fs::remove_file(file).with_context(|| format!("cannot delete {}", file.display()))
    }

    /// Atomically replaces `target` with `tmp`. On failure `tmp` is removed and `target` is left untouched.
    ///
    /// # Errors
    /// [`ReplaceError::InUse`] when the target is locked, [`ReplaceError::Io`] otherwise.
    pub fn replace(&self, tmp: &Path, target: &Path) -> Result<(), ReplaceError> {
        let Err(e) = std::fs::rename(tmp, target) else {
            return Ok(());
        };
        let _ = std::fs::remove_file(tmp);
        // Windows: ERROR_ACCESS_DENIED (5) or ERROR_SHARING_VIOLATION (32) when another handle holds the target.
        if e.kind() == std::io::ErrorKind::PermissionDenied || matches!(e.raw_os_error(), Some(5 | 32)) {
            Err(ReplaceError::InUse)
        } else {
            Err(ReplaceError::Io(e))
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::path::{Path, PathBuf};

    use rusqlite::Connection;

    use super::{CatalogDir, MetaUpdate, ReplaceError, open_read};
    use crate::catalog::schema::{create, read_volume, write_volume};
    use crate::core::model::Volume;

    pub(crate) fn sample_volume(name: &str) -> Volume {
        Volume {
            name: name.to_owned(),
            comment: String::new(),
            archive: false,
            preserved: false,
            serial: Some("A41C-77E2".to_owned()),
            label: Some(name.to_owned()),
            filesystem: Some("NTFS".to_owned()),
            source_path: "E:\\".to_owned(),
            volume_root: Some("E:\\".to_owned()),
            total_bytes: Some(1_863_000_000_000),
            free_bytes: Some(407_000_000_000),
            file_count: 0,
            dir_count: 0,
            skipped_count: 0,
            cataloged_at: 1_791_000_000,
            oxycat_version: "0.1.0".to_owned(),
            ignore: String::new(),
        }
    }

    pub(crate) fn make_catalog(file: &Path, volume: &Volume) {
        let conn = Connection::open(file).unwrap();
        create(&conn).unwrap();
        write_volume(&conn, volume).unwrap();
    }

    fn dir_with(names: &[&str]) -> (tempfile::TempDir, CatalogDir) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = CatalogDir::new(tmp.path().to_path_buf());
        for name in names {
            make_catalog(&dir.path_for(name), &sample_volume(name));
        }
        (tmp, dir)
    }

    #[test]
    fn volume_roundtrip() {
        let (_tmp, dir) = dir_with(&["B1"]);
        let conn = open_read(&dir.path_for("B1")).unwrap();
        assert_eq!(read_volume(&conn).unwrap(), sample_volume("B1"));
    }

    #[test]
    fn list_sorted_by_name() {
        let (_tmp, dir) = dir_with(&["sarterm", "b2", "B1"]);
        let names: Vec<String> = dir.list().catalogs.into_iter().map(|c| c.volume.name).collect();
        assert_eq!(names, ["B1", "b2", "sarterm"]);
    }

    #[test]
    fn find_case_insensitive() {
        let (_tmp, dir) = dir_with(&["Astérix"]);
        assert_eq!(dir.find("ASTÉRIX").unwrap().volume.name, "Astérix");
        assert!(dir.find("other").is_none());
    }

    #[test]
    fn update_meta_persists() {
        let (_tmp, dir) = dir_with(&["B1"]);
        let meta = MetaUpdate { comment: "BD".to_owned(), archive: true, preserved: true, ignore: "*.tmp".to_owned() };
        dir.update_meta(&dir.path_for("B1"), &meta).unwrap();
        let v = dir.find("B1").unwrap().volume;
        assert_eq!((v.comment.as_str(), v.archive, v.preserved, v.ignore.as_str()), ("BD", true, true, "*.tmp"));
    }

    #[test]
    fn version_1_catalog_upgraded_on_listing() {
        let (_tmp, dir) = dir_with(&["Old"]);
        let file = dir.path_for("Old");
        let conn = Connection::open(&file).unwrap();
        conn.execute_batch("ALTER TABLE volume DROP COLUMN ignore_patterns; PRAGMA user_version = 1;").unwrap();
        drop(conn);
        let listing = dir.list();
        assert!(listing.invalid.is_empty(), "{:?}", listing.invalid);
        assert_eq!(listing.catalogs[0].volume.ignore, "");
        let version: i32 = open_read(&file).unwrap().pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version, crate::catalog::schema::SCHEMA_VERSION);
    }

    #[test]
    fn rename_moves_file_and_name() {
        let (_tmp, dir) = dir_with(&["B1"]);
        let new_file = dir.rename(&dir.path_for("B1"), "Bleu").unwrap();
        assert_eq!(new_file, dir.path_for("Bleu"));
        assert!(!dir.path_for("B1").exists());
        assert_eq!(dir.find("Bleu").unwrap().volume.name, "Bleu");
    }

    #[test]
    fn rename_case_only_allowed() {
        let (_tmp, dir) = dir_with(&["b1"]);
        dir.rename(&dir.path_for("b1"), "B1").unwrap();
        assert_eq!(dir.list().catalogs[0].volume.name, "B1");
    }

    #[test]
    fn rename_refuses_existing() {
        let (_tmp, dir) = dir_with(&["B1", "B2"]);
        assert!(dir.rename(&dir.path_for("B1"), "b2").is_err());
        assert_eq!(dir.find("B1").unwrap().volume.name, "B1");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn rename_refuses_case_twin() {
        let (_tmp, dir) = dir_with(&["b1", "x"]);
        make_catalog(&dir.root().join("B1.db"), &sample_volume("B1"));
        assert!(dir.rename(&dir.path_for("x"), "b1").is_err());
        assert!(dir.rename(&dir.path_for("x"), "B1").is_err());
        assert_eq!(dir.list().catalogs.len(), 3);
    }

    #[test]
    fn delete_removes_file() {
        let (_tmp, dir) = dir_with(&["B1"]);
        dir.delete(&dir.path_for("B1")).unwrap();
        assert_eq!(dir.list().catalogs, [] as [super::CatalogInfo; 0]);
    }

    #[test]
    fn foreign_files_reported_invalid() {
        let (tmp, dir) = dir_with(&["B1"]);
        let root = tmp.path();
        std::fs::write(root.join("empty.db"), b"").unwrap();
        Connection::open(root.join("foreign.db")).unwrap().execute_batch("CREATE TABLE t (x);").unwrap();
        let newer = root.join("newer.db");
        make_catalog(&newer, &sample_volume("newer"));
        Connection::open(&newer)
            .unwrap()
            .pragma_update(None, "user_version", crate::catalog::schema::SCHEMA_VERSION + 1)
            .unwrap();
        std::fs::write(root.join("left.db.tmp"), b"partial").unwrap();
        let files = ["empty.db", "foreign.db", "newer.db", "left.db.tmp"];
        let before: Vec<Vec<u8>> = files.iter().map(|f| std::fs::read(root.join(f)).unwrap()).collect();

        let listing = dir.list();

        assert_eq!(listing.catalogs.len(), 1);
        let mut invalid: Vec<PathBuf> = listing.invalid.into_iter().map(|(p, _)| p).collect();
        invalid.sort();
        assert_eq!(invalid, [root.join("empty.db"), root.join("foreign.db"), root.join("newer.db")]);
        let after: Vec<Vec<u8>> = files.iter().map(|f| std::fs::read(root.join(f)).unwrap()).collect();
        assert_eq!(before, after);
    }

    #[test]
    fn replace_swaps_files() {
        let (tmp, dir) = dir_with(&["B1"]);
        let staged = tmp.path().join("B1.db.tmp");
        let mut volume = sample_volume("B1");
        volume.comment = "new".to_owned();
        make_catalog(&staged, &volume);
        dir.replace(&staged, &dir.path_for("B1")).unwrap();
        assert!(!staged.exists());
        assert_eq!(dir.find("B1").unwrap().volume.comment, "new");
    }

    #[test]
    fn replace_in_use_keeps_old() {
        let (tmp, dir) = dir_with(&["B1"]);
        let staged = tmp.path().join("B1.db.tmp");
        make_catalog(&staged, &sample_volume("other"));
        let held = open_read(&dir.path_for("B1")).unwrap();
        let result = dir.replace(&staged, &dir.path_for("B1"));
        drop(held);
        if cfg!(windows) {
            assert!(matches!(result, Err(ReplaceError::InUse)), "{result:?}");
            assert_eq!(dir.find("B1").unwrap().volume.name, "B1");
            assert!(!staged.exists());
        } else {
            assert!(result.is_ok());
        }
    }
}
