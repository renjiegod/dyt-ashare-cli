# Development and release

Use an isolated worktree from fetched origin/main. Read [`RELEASE_SOP.md`](RELEASE_SOP.md) for
the complete version, packaging, publication and post-release acceptance flow. `scripts/test.sh` runs format, Clippy and
behavior tests. `scripts/e2e_update.py` builds real CLI versions, serves an anchored release
channel on loopback, installs, updates, preserves edited Skills, rejects a tampered manifest,
rejects a missing anchor, rejects a corrupted asset, rolls back and performs an unattended
update. On Windows it uses the actual PowerShell
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
uses the static CRT flags in `.cargo/config.toml`). Bundle the executable and the single routed
`dyt-ashare` Skill:

```sh
python3 scripts/package.py --binary target/<target>/release/dytcn --target <target> \
  --base-url https://data.doyoutrade.cloud/releases/dyt-ashare \
  --out dist
```

The package script reads the release version from `Cargo.toml`; an explicit `--version` is
optional and must match it. `scripts/check_release.py` fails the local gate when release code
changes reach a branch without a package version bump.

## The release channel

The channel is one unsigned, content-addressed manifest per target, published at
`https://data.doyoutrade.cloud/releases/dyt-ashare/channel-<target>.json`:

```json
{
  "version": "0.1.6",
  "target": "x86_64-unknown-linux-gnu",
  "bootstrap_url": "https://data.doyoutrade.cloud/releases/dyt-ashare/0.1.6/x86_64-unknown-linux-gnu/dytcn",
  "bootstrap_bytes": 4879144,
  "bootstrap_sha256": "a33db69b…",
  "files": [
    {"path": "dytcn", "url": "…", "bytes": 4879144, "sha256": "a33db69b…"},
    {"path": "skills/dyt-ashare/SKILL.md", "url": "…", "bytes": 5372, "sha256": "a709ae40…"}
  ]
}
```

There is no signature and no expiry. The client trusts the manifest only after the
**repository anchor** pins its exact bytes: `<anchor base>/v<version>/releases/<version>/<target>.sha256`
holds the SHA-256 of the manifest, and it is committed to this repository and published with the
release tag `v<version>`. The anchor base is
`https://raw.githubusercontent.com/renjiegod/dyt-ashare-cli` and can be overridden with the
`DYT_ASHARE_ANCHOR_URL` environment variable; `DYT_ASHARE_RELEASE_URL` overrides the channel
file itself. The anchor check is strict: an unreachable anchor, an unreadable anchor or a hash
mismatch rejects the update, and there is no unanchored fallback.

The trust root is therefore the repository anchor, not the release origin. Compromising the
release origin alone can no longer install an unapproved client, because the origin would also
have to serve a manifest whose bytes match the published anchor. Write access to this repository
is the remaining trust dependency, and the release tag keeps the anchor of a published version
immutable by convention.

Everything else about a release is unchanged: the target must match the running binary, the
version must be a clean semver, the file set must be exactly the executable and the bundled
Skill, every asset carries its size and SHA-256 and must use HTTPS, the downloaded bytes must
match the manifest, versions never move backwards, an installed version directory is never
rewritten, the new binary must pass its `--version` smoke test and its bundled Skill hash must
match the manifest.

The previous channel, a signed envelope with a seven day validity, was retired with release
0.1.6 for the targets that were republished in the anchored format. Clients older than 0.1.6
cannot update themselves any more and must run the installer again. Installed releases written
by those older versions are still readable: their stored metadata is parsed as a legacy envelope
and used for integrity checks and rollback. A target that has not been republished in the
anchored format yet still serves the previous signed channel, and the installers accept both
formats; publish that target in the anchored format to retire its signed channel.

The production release origin is `https://data.doyoutrade.cloud`. Keep it as the `--base-url` origin
for every package; do not use the GitHub Releases page as an asset base URL. Upload immutable
`<version>/<target>/...` assets first, including `release.json`; then publish
`channel-<target>.json` last. Refuse rewriting existing version
assets. Host `install.ps1` and `install.sh` on the same trusted HTTPS origin. Distribution and
backend deployment require a separately authorized release; repository CI never deploys HK.

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
remain visible even when a CLI update succeeds but a Skill cannot be replaced. The canonical Skill
is installed at `~/.agents/skills/dyt-ashare`; detected Agent directories use relative links to
that file, so an anchored update reaches every configured Agent without installing multiple domain
Skills. Modified or foreign files stay in place and are reported as pending.
