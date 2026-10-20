#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
if [[ -z "$(git status --porcelain)" ]]; then
  mkdir -p target
  git rev-parse HEAD > target/verified-commit
fi
