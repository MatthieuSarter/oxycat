//! Data types shared by the catalog store, the search engine and the interface.

/// Description of a cataloged medium: the single row of a catalog's `volume` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    /// Catalog name shown to the user (the file name is derived from it).
    pub name: String,
    /// Free-form comment.
    pub comment: String,
    /// Cathy's "Archive" flag, used by the archived / not archived search modes.
    pub archive: bool,
    /// A preserved catalog is never overwritten nor refreshed.
    pub preserved: bool,
    /// Volume serial number (Windows `XXXX-XXXX`, filesystem UUID on Linux).
    pub serial: Option<String>,
    /// Volume label.
    pub label: Option<String>,
    /// Filesystem type, e.g. `NTFS`.
    pub filesystem: Option<String>,
    /// Cataloged path, as given.
    pub source_path: String,
    /// Root of the volume holding `source_path` when cataloged, e.g. `E:\`.
    pub volume_root: Option<String>,
    /// Volume capacity in bytes.
    pub total_bytes: Option<u64>,
    /// Free space in bytes.
    pub free_bytes: Option<u64>,
    /// Number of cataloged files.
    pub file_count: u64,
    /// Number of cataloged folders.
    pub dir_count: u64,
    /// Number of entries that could not be read.
    pub skipped_count: u64,
    /// When the catalog was made, UTC Unix seconds.
    pub cataloged_at: i64,
    /// Version of Oxycat that made the catalog.
    pub oxycat_version: String,
    /// Ignore patterns (Cathy syntax) used for this catalog, reused when it is refreshed.
    pub ignore: String,
}

/// One file or folder of a catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Row id in the catalog.
    pub id: i64,
    /// Parent folder id; `None` at the top of the cataloged source.
    pub parent_id: Option<i64>,
    /// Name as found on the medium.
    pub name: String,
    /// Whether it is a folder.
    pub is_dir: bool,
    /// Size in bytes; `None` for folders.
    pub size: Option<u64>,
    /// Last modification, UTC Unix seconds.
    pub mtime: Option<i64>,
}

/// A search result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    /// Name of the catalog it was found in.
    pub catalog_name: String,
    /// Path of that catalog file.
    pub catalog_file: std::path::PathBuf,
    /// Archive flag of that catalog.
    pub catalog_archive: bool,
    /// The entry found.
    pub entry: Entry,
    /// Its folded name (for grouping and sorting).
    pub name_fold: String,
    /// Path of its parent folder from the cataloged source, with a leading separator; filled after the search.
    pub path: String,
}

/// Which kinds of entries a search returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum KindFilter {
    /// Files and folders.
    #[default]
    All,
    /// Files only.
    Files,
    /// Folders only.
    Dirs,
}

/// Conditions of a search, already parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    /// Name pattern, possibly combined with `AND` / `OR`.
    pub pattern: super::pattern::Expr,
    /// Match the pattern against the parent folder's name instead of the entry's (Cathy's "Parents" mode).
    pub parents: bool,
    /// Minimum size in bytes (excludes folders).
    pub min_size: Option<u64>,
    /// Maximum size in bytes (excludes folders).
    pub max_size: Option<u64>,
    /// Modified at or after, UTC Unix seconds.
    pub from: Option<i64>,
    /// Modified before (exclusive), UTC Unix seconds.
    pub to: Option<i64>,
    /// Kinds of entries.
    pub kind: KindFilter,
}

impl SearchQuery {
    /// A query matching every entry.
    #[must_use]
    pub fn all() -> SearchQuery {
        SearchQuery {
            pattern: super::pattern::Expr::Leaf(super::pattern::Pattern {
                anchored_start: false,
                anchored_end: false,
                parts: vec![String::new()],
            }),
            parents: false,
            min_size: None,
            max_size: None,
            from: None,
            to: None,
            kind: KindFilter::All,
        }
    }
}
