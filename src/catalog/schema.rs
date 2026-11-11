//! Catalog database schema (documented in `docs/user-guide.md`).

use rusqlite::{Connection, params};

use crate::core::model::Volume;

/// Value of `PRAGMA user_version` for the current schema.
pub const SCHEMA_VERSION: i32 = 1;

const SCHEMA_SQL: &str = "
CREATE TABLE volume (id INTEGER PRIMARY KEY CHECK (id = 1), name TEXT NOT NULL, comment TEXT NOT NULL DEFAULT '',
  archive INTEGER NOT NULL, preserved INTEGER NOT NULL, serial TEXT, label TEXT, filesystem TEXT,
  source_path TEXT NOT NULL, volume_root TEXT, total_bytes INTEGER, free_bytes INTEGER, file_count INTEGER NOT NULL,
  dir_count INTEGER NOT NULL, skipped_count INTEGER NOT NULL, cataloged_at INTEGER NOT NULL,
  oxycat_version TEXT NOT NULL);
CREATE TABLE entries (id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES entries(id), name TEXT NOT NULL,
  name_fold TEXT NOT NULL, is_dir INTEGER NOT NULL, size INTEGER, mtime INTEGER);
CREATE INDEX entries_parent ON entries(parent_id);
";

/// Creates the tables in an empty database, in rollback-journal mode, and stamps the schema version.
///
/// # Errors
/// Any SQLite error.
pub fn create(conn: &Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "journal_mode", "DELETE")?;
    conn.execute_batch(SCHEMA_SQL)?;
    conn.pragma_update(None, "user_version", SCHEMA_VERSION)
}

/// Reads the volume row.
///
/// # Errors
/// Any SQLite error, including a missing table or row.
pub fn read_volume(conn: &Connection) -> rusqlite::Result<Volume> {
    conn.query_row(
        "SELECT name, comment, archive, preserved, serial, label, filesystem, source_path, volume_root, total_bytes,
           free_bytes, file_count, dir_count, skipped_count, cataloged_at, oxycat_version FROM volume WHERE id = 1",
        [],
        |r| {
            Ok(Volume {
                name: r.get(0)?,
                comment: r.get(1)?,
                archive: r.get(2)?,
                preserved: r.get(3)?,
                serial: r.get(4)?,
                label: r.get(5)?,
                filesystem: r.get(6)?,
                source_path: r.get(7)?,
                volume_root: r.get(8)?,
                total_bytes: r.get(9)?,
                free_bytes: r.get(10)?,
                file_count: r.get(11)?,
                dir_count: r.get(12)?,
                skipped_count: r.get(13)?,
                cataloged_at: r.get(14)?,
                oxycat_version: r.get(15)?,
            })
        },
    )
}

/// Writes (inserts or replaces) the volume row.
///
/// # Errors
/// Any SQLite error.
pub fn write_volume(conn: &Connection, v: &Volume) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO volume (id, name, comment, archive, preserved, serial, label, filesystem, source_path,
           volume_root, total_bytes, free_bytes, file_count, dir_count, skipped_count, cataloged_at, oxycat_version)
         VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            v.name,
            v.comment,
            v.archive,
            v.preserved,
            v.serial,
            v.label,
            v.filesystem,
            v.source_path,
            v.volume_root,
            v.total_bytes,
            v.free_bytes,
            v.file_count,
            v.dir_count,
            v.skipped_count,
            v.cataloged_at,
            v.oxycat_version,
        ],
    )
    .map(|_| ())
}
