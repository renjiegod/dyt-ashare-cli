#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
branch=$(git branch --show-current)
[[ -n "$branch" && "$branch" != main ]] || { echo 'Use a feature worktree.' >&2; exit 1; }
[[ -z "$(git status --porcelain)" ]] || { echo 'Commit first.' >&2; exit 1; }
git fetch origin
git merge-base --is-ancestor origin/main HEAD || { echo 'Rebase onto origin/main.' >&2; exit 1; }
if [[ ! -f target/verified-commit || "$(cat target/verified-commit)" != "$(git rev-parse HEAD)" ]]; then scripts/test.sh; fi
gitleaks git --redact --no-banner --log-opts='origin/main..HEAD'
git push -u origin "$branch"
pr=$(gh pr view "$branch" --json number --jq .number)
gh pr checks "$pr" --watch --fail-fast
gh pr merge "$pr" --merge --match-head-commit "$(git rev-parse HEAD)"
git fetch origin
