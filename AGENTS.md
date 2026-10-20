# DYT AShare

Public Rust client and bundled Skills; proprietary service stays in its own repository.
Use a worktree from freshly fetched origin/main. Never commit keys or market data.
Run scripts/test.sh; commit, create a PR, then scripts/land.sh. Do not push main directly.
No production deployment is implied by client development. Native Windows acceptance must exercise installation,
update activation, rollback and the scheduled task before claiming Windows acceptance.
Keep stdout as command JSON; diagnostics use stderr. Client checks never replace server quotas.

## Local validation policy

The owner has disabled GitHub Actions to avoid consuming hosted CI quota. Do not enable or run
GitHub workflows without a new explicit request. Automatic PR/main triggers stay off. Run the
local verification entry, scan secrets and merge through scripts/land.sh; do not wait for or
require GitHub CI. Retain prior native Windows evidence and state its source/configuration.
