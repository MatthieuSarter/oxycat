# Architecture

Oxycat is a single Rust crate building one executable, `oxycat`: the graphical interface without arguments, the
command line with the `catalog` sub-command.

## Modules

| Module | Role | I/O |
|---|---|---|
| `src/core/` | Business logic: search patterns and accent folding, size/date parsing and formatting, ignore rules, file names, refresh decisions, search modes, list sorting/filtering, clipboard and tree text, search form parsing, medium location | **none** (pure functions) |
| `src/catalog/schema.rs` | SQLite schema of a catalog, volume row read/write | SQLite |
| `src/catalog/store.rs` | The catalog directory: list, open (read-only), edit flags, rename, delete, atomic replace | files, SQLite |
| `src/catalog/scan.rs` | Walks a source folder into a temporary database, then atomically replaces the catalog | files, SQLite |
| `src/catalog/query.rs` | Folder contents, ancestors, folder paths, search in one catalog | SQLite |
| `src/search.rs` | Searches many catalogs in parallel and applies the search modes | SQLite, threads |
| `src/platform/` | Volume serial/label/space, mounted volumes, open/reveal/properties (Win32, `/proc`, `statvfs`) | OS |
| `src/config.rs` | Catalog directory resolution, `oxycat.toml` | files, environment |
| `src/i18n.rs` | Languages, translated strings (FR/EN/ES), number and date conventions | — |
| `src/cli.rs` | Command line arguments and the `catalog` command | — |
| `src/gui/` | egui interface: window layout, catalog panel, add dialog and scan progress, Search and Folders tabs, settings and about windows | — |
| `src/main.rs` | Entry point: console attachment on Windows, dispatch to CLI or GUI | — |

## Data flow

```text
medium ──scan.rs──▶ <name>.db.tmp ──atomic rename──▶ <name>.db
                                                        │
                    store.rs (list, flags) ◀────────────┤
                    query.rs (folders) ◀────────────────┤
search.rs (one connection per catalog, rayon pool) ◀────┘
        │
        ▼
core::modes (duplicates, archived…) ─▶ hits ─▶ GUI tables / CLI
```

- A catalog is written once, in a temporary file, with journaling and syncing disabled for speed, then renamed over
  the previous one: a failed or cancelled scan never damages an existing catalog.
- **Connections are never kept open** between operations. On Windows an open SQLite handle prevents the rename that
  replaces a catalog; the GUI therefore opens a catalog for each query (opening costs well under a millisecond).
- Folder browsing reads `entries` through the `parent_id` index: showing any folder is a single indexed query, which
  is what makes double-clicking a search result instant (Cathy re-read its whole catalog file).
- Searches run a `LIKE` on the stored folded name (`name_fold`), one catalog per thread on a pool of
  `max(1, cores − 1)` threads, so the machine stays responsive.

## Threads in the GUI

The interface thread never scans or searches. A scan runs on its own thread and reports progress through atomics
(`ScanControl`); a search runs on a thread that sends its result through a channel (a newer search simply replaces
the receiver). A background thread refreshes, every 3 s, the list of mounted volumes and whether the network shares
(and media without serial number) that catalogs come from are reachable: drives and shares can answer slowly, so the
interface thread never queries them. A medium is located by serial number among mounted volumes, a network share by
its path. The catalog directory is re-listed every 2 s when its files change (e.g. a scheduled `oxycat catalog` run).
