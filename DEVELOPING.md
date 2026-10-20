# Development and release

Use an isolated worktree from fetched origin/main. `scripts/test.sh` runs format, Clippy and
behavior tests. `scripts/e2e_update.py` builds two real CLI versions using an isolated test key,
serves signed releases on loopback, installs, updates, preserves edited Skills, rejects a tampered
manifest, rolls back and performs an unattended update. On Windows it uses the actual PowerShell
installer and verifies the registered scheduled task, then removes that task and temporary PATH.
The acceptance recipe has passed on native Linux, macOS and Windows x64. GitHub Actions
is currently disabled at the owner's request; automatic PR/main triggers are off and local
checks are the merge gate. Do not enable or dispatch the retained workflow without explicit
approval. Windows ARM64 requires its own native acceptance
before claiming that architecture is verified. No production service or user Skills are used by tests.

The backend contract is `/v1/usage`, `/v1/datasets`, `/v1/minutes` and immutable dataset
manifest/download/content routes. Public source contains no backend implementation or data.
Authentication in this version is an operator-issued Key stored locally. Browser customer login,
self-service signup and payment processing are separate follow-ups; the admin GitHub device login
must not be treated as customer login.

For each target, build with `cargo build --release --locked --target <target>` (Windows MSVC
uses the static CRT flags in `.cargo/config.toml`). Bundle the executable and two Skills:

```sh
python3 scripts/package.py --binary target/<target>/release/dyt --target <target> \
  --version 0.1.0 --base-url https://data.doyoutrade.cloud/releases/dyt-ashare \
  --key /secure/location/cn-ed25519.pem --out dist
```

Use the matching Ed25519 private key for `release-public-key.hex`; never commit the private key.
The release custodian generated the current key locally. A trust-key override
`DYT_ASHARE_RELEASE_PUBLIC_KEY` is compile-time only, intended for isolated acceptance builds or
planned product key rotation; an environment variable cannot replace trust in installed binaries.
OpenSSL 3 is needed only by packaging/tests, not by users. macOS LibreSSL is insufficient.

Upload immutable `<version>/<target>/...` assets first, including `release.json`; then publish
`stable-<target>.json` last. Signed payloads expire after 14 days; renew the signed channel
metadata before expiry, including when no binary changes. Refuse rewriting existing version
assets. Host `install.ps1` and `install.sh` on the same trusted HTTPS origin. Distribution and
backend deployment require a separately authorized release; repository CI never deploys HK.
Check the executable public key against the release key before publishing a production artifact.

Native Windows package build should also inspect PE imports to confirm no VC redistributable is
required, and test on a clean supported Windows installation. CI is runtime evidence on GitHub's
runner, not evidence for every customer Windows configuration. Authenticode signing and a
Setup.exe wrapper can be added when distribution requires them.

The version-independent launcher lives in `bin/`. Each update downloads into a sibling staging
directory and activates `active.json` after a version smoke test. Executable directories are
immutable; running processes retain their old images. Update lock serializes writers, status
records last attempt/success/error, offline failures retry after one hour, successful checks after
six hours. Rollback verifies stored signed metadata/hashes and pins auto-updates. Retain previous
versions rather than deleting files still in use. Skills status and actual installed file hashes
remain visible even when a CLI update succeeds but a Skill cannot be replaced.
