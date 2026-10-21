#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
sh -n scripts/install.sh
sh scripts/install.sh --help >/dev/null
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
if [[ -z "$(git status --porcelain)" ]]; then
  mkdir -p target
  git rev-parse HEAD > target/verified-commit
fi
