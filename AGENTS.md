# DYT CN

Public Rust client and bundled Skills; proprietary service stays in its own repository.
Use a worktree from freshly fetched origin/main. Never commit keys or market data.
Run scripts/test.sh; commit, create a PR, then scripts/land.sh. Do not push main directly.
No production deployment is implied by client development. Windows CI must exercise installation,
update activation, rollback and the scheduled task before claiming Windows acceptance.
Keep stdout as command JSON; diagnostics use stderr. Client checks never replace server quotas.
