# Development

## Toolchain

- Rust 1.98.1, pinned in `rust-toolchain.toml` (rustup installs it with `clippy`, `rustfmt` and
  `llvm-tools-preview`); bump it together with `rust-version` in `Cargo.toml`.
- `cargo install --locked cargo-llvm-cov cargo-deny` (the CI pins the versions it installs).
- `cargo deny check` checks licenses, advisories, bans and sources (`deny.toml`); the CI runs it.
- [just](https://just.systems) and the globally installed `repo-linter`.
- On Windows, the Windows SDK (installed with the MSVC build tools): `build.rs` embeds the icon in the executable with
  its resource compiler.
- [uv](https://docs.astral.sh/uv/), only to redraw the icon.

## Recipes

| Recipe | What it does |
|---|---|
| `just lint` | repo-linter, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` |
| `just check-format --fix` | apply `rustfmt` |
| `just test` | `cargo test` |
| `just coverage` | line coverage, HTML report under `.reports/coverage/`, fails below 80 % |
| `just run -- <args>` | run Oxycat (GUI without arguments, CLI with a sub-command) |
| `just build` | release build |
| `just icon` | redraw `assets/icon.png` (window icon) and `assets/icon.ico` (executable icon) from `scripts/make_icon.py` |

## Coverage scope

The coverage gate (80 % minimum, 90 % targeted) applies to the whole crate except:

- `src/gui/` — egui rendering code; its logic (filtering, sorting, clipboard and tree formatting…) lives in
  `src/core/` and is covered there;
- `src/platform/` — thin wrappers around OS calls (Win32, `/proc`, `statvfs`), exercised manually and by the scanner
  tests;
- `src/main.rs` — argument dispatch, covered end to end by `tests/cli.rs` (a separate process, not measured).

## Windows tests in CI

The CI has no Windows runner: its `check-windows` job cross-compiles the test suite for `x86_64-pc-windows-gnu` (the
target of the released `.exe`) and runs it under Wine, so the Win32 code of `src/platform/` and the Windows path
handling are exercised on every push. Wine is close to Windows, not identical (file locking, NTFS details), so the
Windows-specific behaviors still deserve a manual check on a real Windows before a release. To replay it locally on
Linux, with Wine 9 or later (Wine 8 lacks `bcryptprimitives.dll`, which Rust's standard library imports) and a UTF-8
locale (Wine converts file names through it):

```bash
sudo apt-get install gcc-mingw-w64-x86-64 wine wine64 && rustup target add x86_64-pc-windows-gnu
LC_ALL=C.UTF-8 CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc \
  CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUNNER=wine WINEDEBUG=-all cargo test --target x86_64-pc-windows-gnu
```

## Conventions

Lines are at most 120 characters (`rustfmt.toml`). Commit messages follow Conventional Commits with a gitmoji; the
version and changelog are produced by python-semantic-release (`psr.toml`), never by hand.
