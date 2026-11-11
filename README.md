# Oxycat

Oxycat catalogs your storage media (external drives, USB keys, network shares…) and lets you search and browse
those catalogs while the media are disconnected. It is a modern rewrite, in Rust, of
[Cathy](https://www.softpedia.com/get/System/Hard-Disk-Utils/Cathy.shtml), a freeware cataloger unmaintained since 2019:
same organization, same search features, instant browsing.

- One SQLite file per cataloged medium, readable by any SQLite tool (format: [docs/user-guide.md](docs/user-guide.md)).
- Portable: a single executable, catalogs and settings in one directory, nothing in the registry.
- Interface in French, English and Spanish; light, dark and automatic themes.
- Windows first, Linux supported.

Cathy was a great lightweight tools, that I used for almost thirty years. But recently I had a lot of perfomance
issues with it, some operations, like double clicking on a folder from a big catalog in the search results, were
causing my whole system to lag for dozens of seconds. It was time to move on.

## 🤖 AI Usage Disclaimer

This project was built entirely via "vibe coding" using **Claude Opus 5.5**. For me this project is both motivated by
an utilitarian point of view and an educational one: I wanted to test how I can write a complete Rust app without much
knowledge of Rust and its ecosystem.

As an experienced software developer without a background in Rust, I relied on AI assistance for the complete codebase,
documentation included, except some paragraphs of this README file.
.
While the project is functional and tested for its intended use case (ie. my own personal usage), please keep in mind:
- **Code Quality:** Parts of the implementation might not follow idiomatic Rust patterns (*clippy* fans, be gentle!).
- **Security & Performance:** The code has not been audited by an experienced Rust engineer.
- **Contributions:** PRs, refactoring, and idiomatic Rust improvements are extremely welcome!

For those reasons, I won't provide any pre-compiled binary. Build and use it at your own risks.

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
