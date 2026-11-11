#!/usr/bin/env bash
# Refresh Cargo.lock after python-semantic-release stamped the new version into Cargo.toml.
# The release runner may have no Rust toolchain: fall back to editing the lock file's own entry.
set -euo pipefail
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -n 1)
if command -v cargo > /dev/null; then
  cargo update --package oxycat --offline
else
  sed -i "/^name = \"oxycat\"$/{n;s/^version = \".*\"$/version = \"${version}\"/}" Cargo.lock
fi
git add Cargo.lock
