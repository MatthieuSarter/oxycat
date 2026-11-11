# User guide

Oxycat keeps one catalog per storage medium (disk, USB key, folder, network share) and lets you search and browse
them while the media are unplugged. Settings and catalog location: see [configuration.md](configuration.md).

## Window

- **Top bar**: theme (light, dark, automatic) and language (French, English, Spanish, or the system's), settings,
  about.
- **Left panel**: the catalogs. Check the ones a search must cover (new catalogs start checked); filter the list by
  name or comment; *Check all* / *Uncheck all* apply to the filtered list. Click a catalog to see its card at the
  bottom: comment (saved with Enter or when leaving the field), *Archive* and *Preserved* flags, disk usage, counts,
  serial number, source and cataloging date. Double-click a catalog to browse it. Right-click: *Browse*, *Refresh*,
  *Rename*, *Delete*.
- **Search** and **Folders** tabs on the right, with a status bar at the bottom.

## Cataloging a medium

Click **+ Add** (or drop a drive or folder on the window), check the proposed name (the volume label, else the folder
name), add a comment, and click *Catalog*. A progress window shows the entries found; *Cancel* keeps the previous
catalog untouched.

- **Archive** marks catalogs used as the reference by the *Archived* / *Not archived* search modes; it is proposed
  for removable media.
- **Ignore** leaves entries out, Cathy style, `;`-separated: `*.tmp` ignores file names matching the pattern
  (`*` and `?` wildcards, case-insensitive), `\Temp*` (leading backslash) ignores folders.
- Using the name of a catalog made from **another medium or folder** asks before replacing it. A **preserved**
  catalog is never replaced nor refreshed.
- **Refresh** (right-click on a catalog) re-scans the medium, found by its serial number even if it changed drive
  letter; the name, comment and flags are kept.
- Nothing is ever written to the cataloged medium. Unreadable folders are skipped and counted.

## Searching

Type part of a name and press Enter or *Search*. The search covers the checked catalogs and ignores case and accents
(`asterix` finds `Astérix`).

| Pattern | Finds |
|---|---|
| `doc` | names containing "doc" (`*doc` and `doc*` mean the same) |
| `.cbz\|` | names ending with ".cbz" |
| `\|s*doc\|` | names starting with "s" and ending with "doc" (`Solution.DOC`) |
| `\|s*doc` | names starting with "s" and containing "doc" later (`SolDOCument.txt`) |
| `s*doc` | names containing "s" then "doc" (`unSolDOCument.txt`) |

At most one `*` is allowed inside a pattern. An empty pattern matches everything.

**Filters** (button *Filters*):

- **Date**: modification date from / to (both included), as `YYYY-MM-DD`, `YYYY-MM` (first day of the month),
  `YYYY` (January 1st) or `-n` (n days ago), in local time.
- **Size**: minimum / maximum, with decimal units: `3M` = `3000K` = `3000000` bytes, `3G`, `2T`. A size filter
  excludes folders.
- **Type**: all, files only, folders only.
- **Mode**:
  - *Standard*: every match;
  - *Duplicates*: files having the same name **and** size as another file;
  - *Same names* / *Same sizes*: files sharing their name / size with another file;
  - *Singles*: files with no other file of the same name and size;
  - *Parent folder*: entries whose parent folder name matches the pattern (search an author to list their albums);
  - *Not archived*: files of the checked non-archive catalogs that no checked *Archive* catalog contains (same name
    and size) — what still needs a backup;
  - *Archived*: the opposite.

Results are capped by the *Maximum number of results* setting (10 000 by default); the status bar says when the limit
is reached. Click a column header to sort. **Double-click a result to open its folder** in the Folders tab.

## Browsing folders

The Folders tab shows one folder of a catalog: click the breadcrumb or ⬆ (or press Backspace) to go up, double-click
a folder to enter it, ◀ / ▶ (Ctrl+P / Ctrl+N) to show the previous / next catalog of the left list. The status bar
tells whether the medium is plugged in, and where.

## Actions on entries

Select rows (Ctrl+click, Shift+click) and right-click:

- **Open**, **Show in file manager**, **Properties** (Windows) work when the medium is plugged in — Oxycat finds it
  by serial number, whatever its drive letter.
- **Copy** (also Ctrl+C) puts the rows as tab-separated text (name, size in bytes, date, path, volume) in the
  clipboard, ready to paste into a spreadsheet.
- **Export folder tree** (on a folder) saves the list of its sub-folders, indented, to a `.txt` file.

## Command line

See the [README](../README.md#command-line) for `oxycat catalog`, to catalog media from scripts or scheduled tasks.

## Catalog format

Each catalog is a standalone SQLite 3 database named `<name>.db` (characters invalid in file names are replaced by
`_`). It uses the classic rollback journal (never WAL), so a catalog is always a single file you can copy, back up or
open with any SQLite tool. `PRAGMA user_version` holds the schema version (currently `1`); Oxycat lists files with
another version as invalid and never modifies them.

```sql
CREATE TABLE volume (id INTEGER PRIMARY KEY CHECK (id = 1), name TEXT NOT NULL, comment TEXT NOT NULL DEFAULT '',
  archive INTEGER NOT NULL, preserved INTEGER NOT NULL, serial TEXT, label TEXT, filesystem TEXT,
  source_path TEXT NOT NULL, volume_root TEXT, total_bytes INTEGER, free_bytes INTEGER, file_count INTEGER NOT NULL,
  dir_count INTEGER NOT NULL, skipped_count INTEGER NOT NULL, cataloged_at INTEGER NOT NULL,
  oxycat_version TEXT NOT NULL);
CREATE TABLE entries (id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES entries(id), name TEXT NOT NULL,
  name_fold TEXT NOT NULL, is_dir INTEGER NOT NULL, size INTEGER, mtime INTEGER);
CREATE INDEX entries_parent ON entries(parent_id);
```

- `volume` has exactly one row describing the medium: catalog name, comment, *Archive* and *Preserved* flags (0/1),
  serial number (Windows `XXXX-XXXX`, filesystem UUID on Linux), label, filesystem, cataloged path and the root of
  its volume, capacity and free space in bytes, counts of files, folders and unreadable (skipped) entries, the
  cataloging time and the Oxycat version that made it.
- `entries` holds one row per file or folder. Top-level entries of the cataloged path have `parent_id` NULL;
  folders have `is_dir = 1` and `size` NULL; `name_fold` is the name lowercased without accents (used by searches);
  `mtime` is the last modification time.
- All times are UTC Unix seconds.

Example — the 10 biggest files of a catalog with their folder path:

```sql
WITH RECURSIVE dirs(id, path) AS (
  SELECT id, '/' || name FROM entries WHERE parent_id IS NULL AND is_dir = 1
  UNION ALL SELECT e.id, d.path || '/' || e.name FROM entries e JOIN dirs d ON e.parent_id = d.id WHERE e.is_dir = 1)
SELECT coalesce(d.path, '') || '/' || f.name AS file, f.size
FROM entries f LEFT JOIN dirs d ON d.id = f.parent_id
WHERE f.is_dir = 0 ORDER BY f.size DESC LIMIT 10;
```
