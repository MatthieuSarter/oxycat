# Oxycat — design

Date: 2026-11-11 · Status: draft, awaiting approval

Approved UI mockup: [2026-11-11-oxycat-mockup.html](2026-11-11-oxycat-mockup.html) (open it in a browser; it was
rendered inside a dark frame during brainstorming, so its own colors are what matters).

Notation: anything marked **(builder's call)** or **(Claude's call)** was not decided by the owner; the first is left
open, the second is a default chosen during brainstorming that the owner can still override.

## 1. Intent

The owner has used **Cathy 2.33** (freeware by Robert Vasicek, Win32/MFC, unmaintained since 2019) for years to catalog
external storage (hard drives, USB disks, network shares) and search those catalogs while the media are disconnected.
Typical use: "on which disk is that comic book?".

Cathy is getting slow. The worst case: double-clicking a folder in the search results to browse its parent directory
can take tens of seconds and makes the whole PC sluggish meanwhile. Cathy keeps each catalog in a flat binary `.caf`
file (the owner's biggest is 26 MB) and apparently re-reads it on navigation.

**Oxycat** is a from-scratch rewrite in Rust that keeps Cathy's *organization and features* (minus a few dropped ones,
§4) with a modern UI, and makes every interaction instant.

**Success criterion (owner's words):** after one month of use, the owner never had to launch Cathy again.

## 2. Reference: Cathy's behavior

Cathy's executable lives at `S:\Portables\Cathy\Cathy.exe`; its built-in "Tips & Hints" text (extractable from the
exe) is the most precise spec of its behavior. Summary of what Oxycat must reproduce is in §5; when in doubt about a
search-mode semantic, Cathy's behavior is the reference. Existing `.caf` catalogs are **not** imported (owner's
decision): all media will be re-cataloged.

## 3. Goals

- Same feature set as Cathy (§5), same three areas (catalogs / search / folder browsing), modern look.
- Instant interactions: opening a result's folder, browsing, sorting. Search across all catalogs well under a second
  at the owner's scale (a few million entries in total — **estimate, not measured**).
- The UI never freezes and the PC never bogs down: scanning and searching run off the UI thread, with progress and
  cancellation.
- Catalogs are plain SQLite files, documented, so other tools (sqlite3 CLI, DB Browser, Python…) can read them.
- Portable: one directory, no installer, nothing in the registry or `AppData`.
- Windows first; Linux should work too (nice to have, but nothing in the design may make it impossible).
- UI in French, English and Spanish; light, dark and auto (follow OS) themes.
- Open source under **EUPL-1.2**.

## 4. Non-goals and anti-goals

**Non-goals (explicitly dropped by the owner):**

- Reading/importing Cathy `.caf` files.
- MP3 album/song lists, `.m3u` playlist creation, CD eject.
- Deleting files on the media from the app (Cathy's `-dd`).
- Dragging files out of the app to Explorer/other apps.

**Non-goals (Claude's call, YAGNI):** several catalog directories at once (Cathy's `-dir`), Cathy's icon color option
(`-icon`), Cathy's gridlines setting, a "Tips & Hints" window, content hashing, thumbnails, a search CLI.

**Anti-goals — it fails even if it "works" when:**

- Any action freezes the UI or makes the PC sluggish (Cathy's current problem).
- It writes anything to the media being cataloged.
- A failed or cancelled re-scan loses or corrupts the existing catalog.
- It makes any network access of its own (no telemetry, no update check, no remote resource).
- The owner still needs Cathy for something.

## 5. Features

### 5.1 Catalog management (left panel)

- **Add a catalog** from a drive, a sub-directory, or a UNC path; also by dropping a folder/drive onto the window.
  The add dialog proposes the volume label as the catalog name (editable), a comment, the "Archive" flag (pre-set
  from media type as Cathy does: removable → archive; **builder's call** on the exact heuristic), and ignore patterns.
- **Ignore patterns**, Cathy syntax: `;`-separated; `*.tmp` matches file names, `\Temp*` (leading backslash) matches
  directory names. Defaults come from settings.
- **Name uniqueness:** a catalog name maps to its file name. Adding a name that already exists asks for confirmation
  to overwrite, and is refused for a *preserved* catalog.
- **Refresh** a catalog: re-scan its source. Only allowed when the medium is present with the same serial number
  (Cathy's rule); otherwise explain why and suggest "Add".
- **Delete** a catalog (confirmation), **rename** it, edit **comment** and **Archive** / **Preserved** flags inline in
  the details card.
- **Filter** the list by name or comment; checkboxes select which catalogs are searched; "check all / uncheck all".
- **Details card** (bottom of the left panel) for the selected catalog: comment (editable), Archive, Preserved, used /
  total space with a gauge, free space, file and folder counts, serial number, source path, cataloged-at date.
- Double-clicking a catalog opens it in the Folders tab at its root.
- Scanning shows progress (entries found, current path) and can be cancelled. Unreadable folders are skipped and
  counted, not fatal.

### 5.2 Search tab

- **Pattern** (Cathy syntax): case-insensitive substring; `|` anchors start or end of the name (`.cbz|` = extension
  cbz); at most one `*` wildcard inside (`|s*doc|`); `*str` ≡ `str*` ≡ `str`. Matching must also be insensitive to
  accents (Claude's call: owner searches French comic titles; `asterix` should find `Astérix`).
- **Filters** (collapsible "Filtres" panel): date from/to, size min/max, mode, reset button.
  - Size: decimal units as in Cathy (`3M` = `3000K` = 3 000 000 bytes, `3G`), case-insensitive.
  - Date (Claude's call, replaces Cathy's `YYMMDD`): `YYYY-MM-DD`, `YYYY-MM` (= 1st of month), `YYYY` (= Jan 1st), or
    `-n` = n days before today. Interpreted in the user's local time.
  - Type: all / files only / folders only (Claude's call, replaces Cathy's `Length < 0` trick).
- **Modes:** Standard; Duplicates (same name **and** size); Same names; Same sizes; Singles (no other entry with same
  name and size); Parents (match the pattern against the parent folder name); Not archived (entries in the checked
  non-archive catalogs whose name+size is absent from every checked archive catalog); Archived (complement: present
  in some archive catalog).
- Search scope = checked catalogs. Results: name, size, date, path, catalog; sortable by any column; result count is
  capped by a configurable hit limit (Cathy's `-hits`), with a visible "limit reached" notice.
- **Double-click** a result → Folders tab on its parent folder, with the result selected. Must be instant.
- Status bar: result count, catalogs searched, search duration, selection count and total size.

### 5.3 Folders tab

- Browses one catalog: breadcrumb (clickable), up button, Backspace = parent, ◀ / ▶ (Ctrl+P / Ctrl+N) = previous /
  next catalog in the left list.
- Double-click a folder to enter it. Sortable columns: name, size, date.
- Status bar: element count and total size, selection size, whether the medium is currently mounted and where.

### 5.4 Shared list behavior (Search and Folders)

- Multi-selection; **copy** selected rows to the clipboard as tab-separated text with a header row, pasteable into a
  spreadsheet (Claude's call: fixed format instead of Cathy's registry-configurable one).
- Context menu: Open, Show in Explorer (file manager on Linux), Properties (OS properties dialog; **builder's call** on
  Linux), Copy, Export folder tree to `.txt` (Cathy's "Directory tree list").
- Open/Show/Properties need the medium to be mounted. **Locating the medium** (Claude's call, replaces Cathy's `-subs`
  drive-letter substitution): find the mounted volume by serial number first (so a disk that moved from `E:` to `F:`
  still works), then fall back to the original source path. Disabled with an explanation when not found.

### 5.5 Top bar and settings

- Theme selector Light / Dark / Auto (default Auto); language selector FR / EN / ES, initial language inferred from
  the OS locale (fallback English).
- Settings: hit limit, default ignore patterns, date display format, thousands separator. Persisted (§6.3).
- About window: version, EUPL-1.2 license, link to the repository.

### 5.6 Command line

- `oxycat catalog <path> --name <name> [--comment <text>] [--archive|--no-archive] [--ignore <patterns>]` creates or
  refreshes a catalog without the GUI, for scheduled runs (Windows Task Scheduler, cron). Same refresh/overwrite rules
  as the GUI; preserved catalogs are not overwritten. Non-zero exit code and a message on stderr on failure.
- Global option `--catalogs <dir>` (also for the GUI).
- **Builder's call:** one binary that attaches to the parent console when run with a sub-command (Claude's preference,
  keeps "a single exe") vs. a separate `oxycat-cli` binary. Must not flash a console window when the GUI is launched
  from Explorer.

## 6. Technical decisions

### 6.1 Decided by the owner

- **Language:** Rust.
- **GUI toolkit:** **egui** (via `eframe`). Slint was considered and rejected by the owner for licensing reasons.
- **License:** EUPL-1.2. All dependencies must be compatible (MIT/Apache-2.0/BSD/Zlib/public domain are fine).
- **Storage:** one SQLite database per catalog (`<name>.db`).
- **Catalog directory resolution:** `--catalogs <dir>` command-line option, else `OXYCAT_CATALOGS` environment
  variable, else the executable's own directory.
- **Settings file:** `oxycat.toml`, in the catalog directory (next to the `.db` files).

### 6.2 Catalog database (Claude's call on the details)

- `rusqlite` with the `bundled` feature (no system SQLite / DLL).
- Rollback journal (no WAL), so a catalog is always a single self-contained file.
- Schema documented in `docs/` and versioned with `PRAGMA user_version`. Starting point:
  - `volume` (single row): name, comment, archive, preserved, serial, label, filesystem, source path, total and free
    bytes, file count, folder count, cataloged-at (UTC), oxycat version.
  - `entries`: id, parent_id (NULL for root), name, is_dir, size (NULL for folders, or total — **builder's call**),
    mtime (UTC). Index on `parent_id` (folder browsing) — the fix for Cathy's slow double-click.
- **Atomic refresh:** a scan writes to a temporary file in the catalog directory and replaces the old catalog only on
  success; cancel or failure leaves the old one intact.
- Flags edited in the GUI (comment, archive, preserved, name) are written immediately.

### 6.3 Search execution (Claude's call)

- Each checked catalog is queried on its own connection, in parallel on a bounded thread pool (leave at least one core
  free so the PC stays responsive), results merged in memory. No `ATTACH` (10-database default limit).
- Plain scans are expected to be fast enough; an FTS5 trigram index is the escape hatch if measurements say otherwise.
- Accent- and case-insensitive matching needs a normalized name: either a stored normalized column or a custom SQL
  function — **builder's call**.
- Pure-function core: pattern parsing/matching, size and date parsing, ignore-pattern matching, duplicate / single /
  archived set logic are pure functions with no I/O, unit-tested without mocks (owner's global convention).

### 6.4 UI

- `egui_extras::TableBuilder` (virtualized rows) for every list.
- Theme: egui `ThemePreference` (Light / Dark / System). Visual style close to the mockup (builder's call on details).
- i18n: translation files embedded in the binary (e.g. `fluent` or `rust-i18n`, **builder's call**); OS locale via
  `sys-locale`.
- All dates shown in local time; stored in UTC.

### 6.5 Platform specifics (builder's call on implementation)

- Volume serial number, label, filesystem, total/free space: `GetVolumeInformationW` / `GetDiskFreeSpaceExW` on
  Windows; on Linux, filesystem UUID and label from the mount table / `/dev/disk/by-uuid`, and `statvfs`.
- Open / show in file manager: Explorer on Windows, `xdg-open` (or equivalent) on Linux.

## 7. Project conventions (from the owner's global rules)

- Docs: `README.md`, `AGENTS.md`, `docs/architecture.md`, `docs/development.md`, `docs/configuration.md`,
  `docs/user-guide.md` (incl. the catalog schema), kebab-case under `docs/`. Code and docs in English; the UI is
  translated.
- `justfile` with useful recipes; `just lint` runs repo-linter (globally installed: not a Python project →
  `use_global_repo_linter := "true"` in `justfile-custom.just`), plus `cargo fmt --check` and `cargo clippy`.
  Max line length 120 (`rustfmt.toml`).
- Tests: `cargo test`, coverage gated at 80 % minimum, target 90 % (e.g. `cargo llvm-cov`). Reports under `.reports/`
  (git-ignored). `.superpowers/` git-ignored too.
- Git: branch `main`, Conventional Commits with gitmoji, versions/changelog from Conventional Commits (never bumped by
  hand; tool choice for a Rust project is **builder's call**). Hosted on Forgejo; CI workflows generated by
  repo-linter's `forgejo-workflows` rule where it covers Rust, custom workflow otherwise (build Windows + Linux).

## 8. Further calls (Claude's, from the builder-readiness review)

- **Scanning:** hidden and system files are cataloged (as Cathy does); symbolic links, junctions and reparse points are
  recorded but never followed (no loops, no double counting).
- **Refresh keeps metadata:** name, comment, Archive and Preserved survive a refresh; only volume stats and entries
  change. A UNC/network source has no reliable serial number: refresh then only checks that the source path exists.
- **File names:** a catalog name containing characters invalid in file names is sanitized for the `.db` file name;
  the display name is the one stored in `volume`.
- **Concurrent use:** the GUI notices when a `.db` file is added, replaced or removed (e.g. by a scheduled CLI run)
  and reloads the catalog list; a catalog being refreshed by the GUI cannot be refreshed again until done.
- **Defaults:** hit limit 10 000; date and number display follow the UI language's conventions.
- **Read-only catalog directory:** the app still opens and searches; settings changes and catalog edits show an
  error instead of failing silently.

## 9. Left to the builder

Crate choices not fixed above (directory walking, i18n, CLI parsing, error handling), module layout, exact visual
styling, the archive-flag heuristic, folder size semantics, accent-folding implementation, release packaging, and the
order in which features are delivered — as long as §3 and §4 hold.
