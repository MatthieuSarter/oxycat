# Rust project: no project-local Python environment, so use the globally installed repo-linter.
use_global_repo_linter := "true"

# Run repo-linter, then rustfmt and clippy checks
[group("Misc")]
lint fix="" extra_opts=lint_extra_opts:
    just _repo_linter lint {{ fix }} {{ extra_opts }}
    just check-format
    just check

# Run clippy on all targets, warnings as errors
[group("Quality")]
check fix="":
    cargo clippy --all-targets {{ if fix != "" { "--fix --allow-dirty" } else { "" } }} -- -D warnings

# Check (or apply, with --fix) the code format
[group("Quality")]
check-format fix="":
    cargo fmt {{ if fix != "" { "" } else { "--check" } }}

# Run the test suite
[group("Testing")]
[positional-arguments]
test *args:
    cargo test "$@"

# Line coverage report (gated at 80%), excluding the GUI, platform and entry-point shells
[group("Testing")]
coverage:
    cargo llvm-cov --ignore-filename-regex 'src[\/](gui|platform)[\/]|main\.rs' --fail-under-lines 80 --html --output-dir .reports/coverage
    cargo llvm-cov report --ignore-filename-regex 'src[\/](gui|platform)[\/]|main\.rs' --summary-only

# Run oxycat with optional arguments
[positional-arguments]
run *args:
    cargo run -- "$@"

# Build the release binary
build:
    cargo build --release

# Redraw the icon assets (assets/icon.png and assets/icon.ico)
icon:
    uv run --script scripts/make_icon.py
