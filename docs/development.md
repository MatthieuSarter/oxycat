# Development

## Toolchain

- Rust stable 1.98 or later (`rustup`), with the `llvm-tools-preview` component for coverage.
- `cargo install cargo-llvm-cov cargo-deny`.
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

## Conventions

Lines are at most 120 characters (`rustfmt.toml`). Commit messages follow Conventional Commits with a gitmoji; the
version and changelog are produced by python-semantic-release (`psr.toml`), never by hand.
