# Oxycat

Oxycat catalogs your storage media (external drives, USB keys, network shares…) and lets you search and browse
those catalogs while the media are disconnected. It is a modern rewrite, in Rust, of
[Cathy](https://www.softpedia.com/get/System/Hard-Disk-Utils/Cathy.shtml), a freeware cataloger unmaintained since 2019:
same organization, same search features, instant browsing.

- One SQLite file per cataloged medium, readable by any SQLite tool (format: [docs/user-guide.md](docs/user-guide.md)).
- Portable: a single executable, catalogs and settings in one directory, nothing in the registry.
- Interface in French, English and Spanish; light, dark and automatic themes.
- Windows first, Linux supported.

## Build and run

Requires a stable Rust toolchain (1.98+).

```bash
cargo build --release
```

The binary is `target/release/oxycat` (`oxycat.exe` on Windows). Copy it wherever you like; by default it reads and
writes its catalogs in its own directory (see [docs/configuration.md](docs/configuration.md)).

## Command line

Without arguments Oxycat opens its window. The `catalog` command creates or refreshes a catalog without it, e.g. from
a scheduled task:

```bash
oxycat catalog E:\ --name B1 --comment "Blue USB disk" --archive --ignore "*.tmp; \Temp*"
```

- Running it again on the same medium and folder refreshes the catalog (comment, flags and ignore patterns are kept
  unless given).
- A name already used by another medium or folder is refused unless `--overwrite` is given; a *preserved* catalog is
  never overwritten.
- `--catalogs <dir>` (or the `OXYCAT_CATALOGS` variable) selects the catalog directory.
- Exit code 0 on success, 1 on failure (message on stderr), 2 on invalid arguments.

Windows Task Scheduler example (action "Start a program"): program `C:\Tools\Oxycat\oxycat.exe`, arguments
`catalog \\nas\photos --name Photos`. When run from a terminal on Windows, the prompt may come back before Oxycat's
message is printed: Oxycat is a windowed application that borrows the terminal's console.

## Documentation

- [User guide](docs/user-guide.md)
- [Configuration](docs/configuration.md)
- [Architecture](docs/architecture.md)
- [Development](docs/development.md)

## License

[EUPL-1.2](LICENSE).
