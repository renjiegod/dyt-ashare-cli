# Unsigned Release Channel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the signed-and-expiring release channel with an unsigned, content-addressed channel manifest pinned by a repository anchor, so a release never needs a weekly re-sign and the release origin can no longer install an unapproved client.

**Architecture:** The channel file becomes a flat JSON manifest (`channel-<target>.json`) that lists every asset with its size and SHA-256; it carries no signature and no expiry. The client trusts the manifest only after its exact bytes match a repository anchor (`releases/<version>/<target>.sha256`, published with the release tag `v<version>`); a mismatch or an unreachable anchor rejects the update. Every other release check (target, version shape, file set, asset size/hash, HTTPS URLs, post-download hash, monotonic version, immutable version directory, smoke test, bundled-Skill hash) stays exactly as it is.

**Tech Stack:** Rust 2024 (clap, serde_json, reqwest blocking, sha2, semver), POSIX shell (`scripts/install.sh`), PowerShell (`scripts/install.ps1`), Python 3 (`scripts/package.py`, `scripts/e2e_update.py`).

**Spec:** This plan is the spec. The durable contract it produces lives in `DEVELOPING.md` (how the channel works) and `RELEASE_SOP.md` (how to publish it).

## Global Constraints

- Client stdout stays pure command JSON; diagnostics and errors go to stderr. Never print a key, a token, or a URL query string.
- The release origin is `https://data.doyoutrade.cloud`; release assets live under `https://data.doyoutrade.cloud/releases/dyt-ashare/`.
- The channel manifest for target T is published at `https://data.doyoutrade.cloud/releases/dyt-ashare/channel-<T>.json`.
- The repository anchor base is `https://raw.githubusercontent.com/renjiegod/dyt-ashare-cli`; the anchor for version V and target T is fetched from `<base>/v<V>/releases/<V>/<T>.sha256`.
- The anchor is strict: an unreachable anchor, an unparsable anchor, or a hash mismatch rejects the update. There is no fallback to an unanchored channel.
- Anchor base override: runtime env `DYT_ASHARE_ANCHOR_URL`, compile-time `DYT_ASHARE_ANCHOR_URL` (build-time constant), default `https://raw.githubusercontent.com/renjiegod/dyt-ashare-cli`.
- Channel file override: runtime env `DYT_ASHARE_RELEASE_URL` (full file URL), default `https://data.doyoutrade.cloud/releases/dyt-ashare/channel-<T>.json`.
- The Ed25519 release key, `release-public-key.hex`, and the `ed25519-dalek` dependency are removed from this change.
- Legacy installed releases (0.1.3 - 0.1.5) keep working for `validate_installed` and `dytcn update rollback`: their stored `release.json` is parsed as a legacy envelope and its payload is used for file hashes. No signature check is possible or performed for those files.
- The release version for this change is `0.1.6`; `Cargo.toml` and `Cargo.lock` must agree.
- The local gate is `scripts/test.sh`; the release acceptance script is `scripts/e2e_update.py` (needs `DYT_OPENSSL` only for the legacy path and is no longer key-dependent); the merge entry is `scripts/land.sh`.
- Never commit market data, API keys, or the release private key.

---

### Task 1: Channel manifest parsing and strict anchor verification

**Files:**
- Modify: `src/update.rs`
- Test: `src/tests.rs`

**Interfaces:**
- Consumes: `state::hash(&[u8]) -> String`, `state::verified(&Path, u64, &str) -> bool`, `api::bytes(Response, u64) -> Result<Vec<u8>, ServiceError>`, `api::http() -> Result<Client>`.
- Produces:
  - `pub struct Manifest { version: String, target: String, expires_at: Option<i64>, files: Vec<Asset> }` (`expires_at` optional; absent in the new format).
  - `pub fn channel_manifest(raw: &[u8]) -> Result<Manifest>` - parses the flat channel format only; rejects a legacy envelope.
  - `pub fn installed_manifest(raw: &[u8]) -> Result<Manifest>` - parses the flat format, or a legacy envelope payload (`payload` string) ignoring its signature.
  - `pub fn validate_manifest(m: &Manifest, target: &str) -> Result<()>` - target match, clean semver, exact file set, asset size/hash/URL checks.
  - `pub fn check_anchor(raw: &[u8], anchor: &[u8]) -> Result<()>` - pure comparison of manifest bytes against anchor text.
  - `pub fn anchor_url(version: &str, target: &str) -> String`.
  - `pub fn channel_url(target: &str) -> String`.
  - `pub fn anchor_base() -> String`.

- [ ] **Step 1: Write the failing tests**

Add to `src/tests.rs`:

```rust
#[test]
fn channel_manifest_parses_the_flat_format_without_signature_or_expiry() {
    let raw = br#"{"version":"0.1.6","target":"x86_64-unknown-linux-gnu","bootstrap_url":"https://data.doyoutrade.cloud/releases/dyt-ashare/0.1.6/x86_64-unknown-linux-gnu/dytcn","bootstrap_bytes":10,"bootstrap_sha256":"0000000000000000000000000000000000000000000000000000000000000000","files":[{"path":"dytcn","url":"https://data.doyoutrade.cloud/releases/dyt-ashare/0.1.6/x86_64-unknown-linux-gnu/dytcn","bytes":10,"sha256":"0000000000000000000000000000000000000000000000000000000000000000"},{"path":"skills/dyt-ashare/SKILL.md","url":"https://data.doyoutrade.cloud/releases/dyt-ashare/0.1.6/x86_64-unknown-linux-gnu/skills/dyt-ashare/SKILL.md","bytes":10,"sha256":"1111111111111111111111111111111111111111111111111111111111111111"}]}"#;
    let m = update::channel_manifest(raw).unwrap();
    assert_eq!(m.version, "0.1.6");
    assert_eq!(m.expires_at, None);
    assert_eq!(m.files.len(), 2);
}

#[test]
fn legacy_installed_metadata_still_parses_for_rollback() {
    let payload = "{\"version\":\"0.1.5\",\"target\":\"x86_64-unknown-linux-gnu\",\"expires_at\":1793201467,\"files\":[{\"path\":\"dytcn\",\"url\":\"https://example.com/dytcn\",\"bytes\":10,\"sha256\":\"0000000000000000000000000000000000000000000000000000000000000000\"}]}";
    let envelope = json!({"payload": payload, "signature": "00"});
    let m = update::installed_manifest(&serde_json::to_vec(&envelope).unwrap()).unwrap();
    assert_eq!(m.version, "0.1.5");
    assert_eq!(m.expires_at, Some(1793201467));
}

#[test]
fn manifest_validation_rejects_a_wrong_target_and_a_broken_file_set() {
    let asset = |path: &str| json!({"path": path, "url": "https://data.doyoutrade.cloud/x", "bytes": 10, "sha256": "0000000000000000000000000000000000000000000000000000000000000000"});
    let m = update::Manifest {
        version: "0.1.6".into(),
        target: "x86_64-unknown-linux-gnu".into(),
        expires_at: None,
        files: vec![
            serde_json::from_value(asset("dytcn")).unwrap(),
            serde_json::from_value(asset("skills/dyt-ashare/SKILL.md")).unwrap(),
        ],
    };
    assert!(update::validate_manifest(&m, "x86_64-unknown-linux-gnu").is_ok());
    assert!(update::validate_manifest(&m, "aarch64-apple-darwin").is_err());
    let mut short = m.clone();
    short.files.pop();
    assert!(update::validate_manifest(&short, "x86_64-unknown-linux-gnu").is_err());
}

#[test]
fn anchor_comparison_accepts_only_the_exact_manifest_bytes() {
    let raw = b"{\"version\":\"0.1.6\"}";
    let digest = state::hash(raw);
    assert!(update::check_anchor(raw, format!("{digest}  channel-x86_64-unknown-linux-gnu.json\n").as_bytes()).is_ok());
    assert!(update::check_anchor(raw, format!("{}  channel-x86_64-unknown-linux-gnu.json\n", "0".repeat(64)).as_bytes()).is_err());
    assert!(update::check_anchor(raw, b"").is_err());
    assert!(update::check_anchor(raw, b"not-a-digest\n").is_err());
}
```

- [ ] **Step 2: Run the tests and verify they fail**

Run: `cargo test --locked channel_manifest 2>&1 | tail -20`
Expected: compile errors - `update::channel_manifest`, `update::installed_manifest`, `update::validate_manifest`, `update::check_anchor` and the `Manifest` fields do not exist yet.

- [ ] **Step 3: Implement the parsing and validation**

In `src/update.rs` replace the `Envelope` struct and `verify()` with:

```rust
#[derive(Serialize, Deserialize, Clone)]
pub struct Manifest {
    pub version: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    pub files: Vec<Asset>,
}

/// The channel manifest is an unsigned, content-addressed description of a release.
pub fn channel_manifest(raw: &[u8]) -> Result<Manifest> {
    let m: Manifest = serde_json::from_slice(raw).context("Invalid release channel manifest")?;
    Ok(m)
}

/// Installed releases written before 0.1.6 are legacy signed envelopes. Their
/// payload still describes the installed files, so rollback and integrity checks
/// keep working; the signature is not checked because no trust key is embedded.
pub fn installed_manifest(raw: &[u8]) -> Result<Manifest> {
    let value: Value = serde_json::from_slice(raw).context("Invalid installed release metadata")?;
    if let Some(payload) = value.get("payload").and_then(Value::as_str) {
        let m: Manifest = serde_json::from_str(payload).context("Invalid legacy release payload")?;
        return Ok(m);
    }
    channel_manifest(raw)
}

pub fn validate_manifest(m: &Manifest, target: &str) -> Result<()> {
    let v = semver::Version::parse(&m.version)?;
    if !v.pre.is_empty() || v.to_string() != m.version || m.target != target {
        bail!("Release target or version is invalid");
    }
    let expected = [binary().to_string(), "skills/dyt-ashare/SKILL.md".into()];
    let paths = m
        .files
        .iter()
        .map(|f| f.path.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    if m.files.len() != expected.len() || paths != expected.iter().map(String::as_str).collect() {
        bail!("Release file set is invalid");
    }
    for f in &m.files {
        if f.bytes == 0
            || f.bytes > MAX_FILE
            || f.sha256.len() != 64
            || !f.sha256.bytes().all(|c| c.is_ascii_hexdigit())
        {
            bail!("Release asset size or hash is invalid");
        }
        state::secure_url(&url::Url::parse(&f.url)?)?;
    }
    Ok(())
}

/// The repository anchor pins the exact channel bytes. A mismatch or an
/// unreadable anchor rejects the update; there is no unanchored fallback.
pub fn check_anchor(raw: &[u8], anchor: &[u8]) -> Result<()> {
    let expected = anchor
        .split_whitespace()
        .next()
        .context("Release anchor is empty")?;
    if expected.len() != 64 || !expected.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("Release anchor is invalid");
    }
    if expected != state::hash(raw) {
        bail!("Release channel anchor mismatch");
    }
    Ok(())
}
```

- [ ] **Step 4: Add the channel and anchor URLs**

Add to `src/update.rs`:

```rust
pub const ANCHOR_BASE: &str = "https://raw.githubusercontent.com/renjiegod/dyt-ashare-cli";
pub const RELEASE_BASE: &str = "https://data.doyoutrade.cloud/releases/dyt-ashare";

pub fn channel_url(target: &str) -> String {
    std::env::var("DYT_ASHARE_RELEASE_URL")
        .unwrap_or_else(|_| format!("{RELEASE_BASE}/channel-{target}.json"))
}

pub fn anchor_base() -> String {
    std::env::var("DYT_ASHARE_ANCHOR_URL")
        .ok()
        .or_else(|| option_env!("DYT_ASHARE_ANCHOR_URL").map(str::to_owned))
        .unwrap_or_else(|| ANCHOR_BASE.to_owned())
}

pub fn anchor_url(version: &str, target: &str) -> String {
    format!("{}/v{version}/releases/{version}/{target}.sha256", anchor_base())
}
```

- [ ] **Step 5: Wire the anchor into the update path and the status output**

In `src/update.rs`:

```rust
fn verify_anchor(raw: &[u8], m: &Manifest, target: &str) -> Result<()> {
    let url = url::Url::parse(&anchor_url(&m.version, target))?;
    state::secure_url(&url)?;
    let anchor = api::bytes(
        api::http()?
            .get(url)
            .send()
            .context("Release anchor is unavailable")?,
        4096,
    )
    .map_err(|error| anyhow::anyhow!("Release anchor is unavailable: {error}"))?;
    check_anchor(raw, &anchor)
}
```

Call `verify_anchor(raw, &m, TARGET)?` in `apply()` right after `validate_manifest(&m, TARGET)?`, and in `run()` replace the manifest URL with `channel_url(TARGET)`:

```rust
let u = url::Url::parse(&channel_url(TARGET))?;
```

In `status()` add the channel description:

```rust
json!({"installed":active(root).ok().flatten(),"checks":state::read::<Value>(&root.join("update-status.json")).ok(),"automatic_updates_enabled":!root.join("updates-disabled").exists(),"target":TARGET,"channel":{"url":channel_url(TARGET),"anchor_base":anchor_base(),"verification":"sha256+anchor"}})
```

In `validate_installed()` replace the signed verification with:

```rust
let raw = fs::read(dir.join("release.json"))?;
let m = installed_manifest(&raw)?;
```

- [ ] **Step 6: Run the tests and verify they pass**

Run: `cargo test --locked 2>&1 | tail -25`
Expected: all tests pass, including the four new ones.

- [ ] **Step 7: Commit**

```bash
git add src/update.rs src/tests.rs
git commit -m "feat: pin the release channel with a repository anchor"
```

---

### Task 2: Remove the release signing key

**Files:**
- Modify: `Cargo.toml`, `build.rs`
- Delete: `release-public-key.hex`
- Modify: `src/update.rs` (remove `public()`, `verify()`, the `Envelope` struct, the `ed25519_dalek` import and the `hex` import if unused)

**Interfaces:**
- Consumes: nothing.
- Produces: a client with no embedded release key; `cargo build` no longer depends on `DYT_ASHARE_RELEASE_PUBLIC_KEY`.

- [ ] **Step 1: Remove the key material and dependency**

Delete `release-public-key.hex`. In `Cargo.toml` remove the line `ed25519-dalek = "2"`. In `build.rs` remove the `DYT_ASHARE_RELEASE_PUBLIC_KEY` rerun line, leaving:

```rust
fn main() {
    println!(
        "cargo:rustc-env=DYT_TARGET={}",
        std::env::var("TARGET").unwrap_or_default()
    );
}
```

In `src/update.rs` remove `use ed25519_dalek::{Signature, VerifyingKey};`, the `Envelope` struct, `fn public()`, and `fn verify()`.

- [ ] **Step 2: Refresh the lockfile and build**

Run: `cargo check 2>&1 | tail -5 && cargo build 2>&1 | tail -5`
Expected: no dependency on `ed25519-dalek`; `Cargo.lock` no longer lists it.

- [ ] **Step 3: Run the whole test suite**

Run: `cargo test --locked 2>&1 | tail -25`
Expected: all tests pass.

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock build.rs release-public-key.hex src/update.rs
git commit -m "refactor: drop the release signing key from the client"
```

---

### Task 3: Publish an unsigned channel manifest and its anchor

**Files:**
- Modify: `scripts/package.py`
- Test: `scripts/e2e_update.py` (Task 5 exercises it)

**Interfaces:**
- Consumes: `--binary <path> --target <target> --version <version> --base-url <url> --out <dir>`; the repository `skills/*/SKILL.md` tree.
- Produces in `--out`:
  - `<version>/<target>/dytcn` and `<version>/<target>/skills/dyt-ashare/SKILL.md` (immutable assets)
  - `<version>/<target>/release.json` (the channel manifest bytes)
  - `channel-<target>.json` (the same manifest bytes)
  - `releases/<version>/<target>.sha256` (the repository anchor: `<sha256>  channel-<target>.json\n`)
  - `install.sh`, `install.ps1`

- [ ] **Step 1: Rewrite the packaging script**

Replace `scripts/package.py` with:

```python
#!/usr/bin/env python3
"""Package a CLI and Skills into an unsigned channel manifest and its repository anchor."""
import argparse, hashlib, json, os, pathlib, shutil
try:
    import tomllib
except ModuleNotFoundError:
    tomllib = None
p=argparse.ArgumentParser()
p.add_argument('--binary',required=True,type=pathlib.Path)
p.add_argument('--target',required=True)
p.add_argument('--version',help='Release version; defaults to Cargo.toml and must match it when supplied')
p.add_argument('--base-url',required=True)
p.add_argument('--out',required=True,type=pathlib.Path)
a=p.parse_args()
root=pathlib.Path(__file__).resolve().parent.parent
if tomllib is not None:
    package_version=tomllib.loads((root/'Cargo.toml').read_text(encoding='utf-8'))['package']['version']
else:
    package_version=next(line.split('=',1)[1].strip().strip('"') for line in (root/'Cargo.toml').read_text(encoding='utf-8').splitlines() if line.startswith('version ='))
version=a.version or package_version
if version != package_version:
    raise SystemExit(f'--version {version} does not match Cargo.toml version {package_version}; bump Cargo.toml first')
prefix=a.out/version/a.target
prefix.mkdir(parents=True,exist_ok=True)
files=[]
for name,src in [('dytcn.exe' if 'windows' in a.target else 'dytcn',a.binary)]+[(str(s.relative_to(root)).replace(os.sep,'/'),s) for s in sorted((root/'skills').glob('*/SKILL.md'))]:
    dest=prefix/name;dest.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(src,dest)
    data=dest.read_bytes()
    files.append(dict(path=name,url=f'{a.base_url.rstrip("/")}/{version}/{a.target}/{name}',bytes=len(data),sha256=hashlib.sha256(data).hexdigest()))
bootstrap=files[0]
manifest=dict(version=version,target=a.target,bootstrap_url=bootstrap['url'],bootstrap_bytes=bootstrap['bytes'],bootstrap_sha256=bootstrap['sha256'],files=files)
# The manifest is deterministic: the same artifacts always produce the same
# bytes, and therefore the same repository anchor.
raw=json.dumps(manifest,indent=2,sort_keys=True).encode()
(prefix/'release.json').write_bytes(raw)
(a.out/f'channel-{a.target}.json').write_bytes(raw)
anchor=a.out/'releases'/version
anchor.mkdir(parents=True,exist_ok=True)
(anchor/f'{a.target}.sha256').write_text(f"{hashlib.sha256(raw).hexdigest()}  channel-{a.target}.json\n")
for name in ['install.sh', 'install.ps1']:
    shutil.copy2(root/'scripts'/name,a.out/name)
print(a.out/f'channel-{a.target}.json')
```

- [ ] **Step 2: Verify the output is deterministic and the anchor matches**

Run:
```bash
python3 scripts/package.py --binary target/aarch64-apple-darwin/release/dytcn --target aarch64-apple-darwin --base-url https://data.doyoutrade.cloud/releases/dyt-ashare --out /tmp/pkg-a && python3 scripts/package.py --binary target/aarch64-apple-darwin/release/dytcn --target aarch64-apple-darwin --base-url https://data.doyoutrade.cloud/releases/dyt-ashare --out /tmp/pkg-b && diff -r /tmp/pkg-a /tmp/pkg-b && shasum -a 256 /tmp/pkg-a/channel-aarch64-apple-darwin.json && cat /tmp/pkg-a/releases/*/aarch64-apple-darwin.sha256
```
Expected: no diff output, and the printed digest equals the anchor's first field.

- [ ] **Step 3: Commit**

```bash
git add scripts/package.py
git commit -m "build: publish an unsigned channel manifest and its repository anchor"
```

---

### Task 4: Install from the channel manifest

**Files:**
- Modify: `scripts/install.sh`
- Modify: `scripts/install.ps1`

**Interfaces:**
- Consumes: the channel manifest at `https://data.doyoutrade.cloud/releases/dyt-ashare/channel-<target>.json` (override with `DYT_ASHARE_RELEASE_URL`).
- Produces: an installed CLI; the anchor is verified later by `dytcn update --manifest`.

- [ ] **Step 1: Point the shell installer at the channel file**

In `scripts/install.sh` change:

```sh
task_url="${DYT_ASHARE_RELEASE_URL:-https://data.doyoutrade.cloud/releases/dyt-ashare/stable-$target.json}"
```

to:

```sh
task_url="${DYT_ASHARE_RELEASE_URL:-https://data.doyoutrade.cloud/releases/dyt-ashare/channel-$target.json}"
```

The `bootstrap_url`, `bootstrap_bytes` and `bootstrap_sha256` parsing stays: the new manifest keeps those flat fields.

- [ ] **Step 2: Point the PowerShell installer at the channel file and the flat manifest**

In `scripts/install.ps1` change the default manifest URL to `channel-$target.json` and parse the root object:

```powershell
$manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
```

Remove the `$envelope = ...` line and the `$manifest = $envelope.payload | ConvertFrom-Json` line. Replace the comment on line 16 with:

```powershell
# TLS authenticates the channel file. The CLI verifies the repository anchor before installing.
```

Replace the failure message `throw 'Signed bundle installation failed'` with `throw 'Channel bundle installation failed'`.

- [ ] **Step 3: Check both installers parse the new manifest**

Run:
```bash
sh -n scripts/install.sh && sh scripts/install.sh --help >/dev/null && python3 -c "
import json,pathlib
m=json.loads(pathlib.Path('/tmp/pkg-a/channel-aarch64-apple-darwin.json').read_text())
assert m['target']=='aarch64-apple-darwin' and m['bootstrap_bytes']>0 and len(m['bootstrap_sha256'])==64
assert [f['path'] for f in m['files']]==['dytcn','skills/dyt-ashare/SKILL.md']
print('manifest shape ok')"
```
Expected: `manifest shape ok` and no shell syntax errors.

- [ ] **Step 4: Commit**

```bash
git add scripts/install.sh scripts/install.ps1
git commit -m "feat: install from the unsigned channel manifest"
```

---

### Task 5: Acceptance script for the anchored channel

**Files:**
- Modify: `scripts/e2e_update.py`

**Interfaces:**
- Consumes: `scripts/package.py` output; a loopback HTTP server that serves the release tree, `channel-<target>.json` and the anchor path `<version>/releases/<version>/<target>.sha256`.
- Produces: the PASS line printed by the script.

- [ ] **Step 1: Rewrite the acceptance script**

Key changes to `scripts/e2e_update.py`:

1. Drop the OpenSSL key generation and the `DYT_ASHARE_RELEASE_PUBLIC_KEY` build variable; package with `scripts/package.py` without `--key`.
2. Serve the anchor: after packaging each version, write the anchor file into the served tree at `web/v<version>/releases/<version>/<target>.sha256` (copy `dist/releases/<version>/<target>.sha256`).
3. Point the CLI at the loopback server with `DYT_ASHARE_RELEASE_URL=<base>/channel-<target>.json` and `DYT_ASHARE_ANCHOR_URL=<base>`.
4. Replace the "tampered manifest is rejected" case with an anchor case: serve a modified manifest whose bytes no longer match the anchor and assert the update fails with an anchor error; and remove the anchor file and assert the update fails as well.

The script must end with:

```python
print('PASS: installation, activation, edited Skills, anchor verification, rollback and unattended update')
```

- [ ] **Step 2: Run the acceptance script**

Run: `python3 scripts/e2e_update.py 2>&1 | tail -10`
Expected: the PASS line.

- [ ] **Step 3: Commit**

```bash
git add scripts/e2e_update.py
git commit -m "test: verify the anchored release channel end to end"
```

---

### Task 6: Document the new channel

**Files:**
- Modify: `DEVELOPING.md`
- Modify: `RELEASE_SOP.md`
- Modify: `README.md`

**Interfaces:**
- Consumes: the format produced by Task 3.
- Produces: the durable contract for operators and reviewers.

- [ ] **Step 1: Rewrite the release section of `DEVELOPING.md`**

Document: the channel file name and shape, the anchor URL shape, that the anchor is strict, that the release origin plus the repository anchor form the trust root, that the release origin alone can no longer install an unapproved client, and that clients older than 0.1.6 must reinstall because the previous signed channel is gone.

- [ ] **Step 2: Rewrite `RELEASE_SOP.md`**

New order: bump the version, local gate, build both targets, package, commit the anchors to the repository and tag `v<version>`, upload the immutable version directory, upload `channel-<target>.json` last, acceptance. Remove the private-key steps, the "re-sign before expiry" step and the "validity is 7 days" wording.

- [ ] **Step 3: Update `README.md`**

State the update trust model in one short paragraph and add a migration note: versions 0.1.6 and later install from the channel manifest; clients on 0.1.5 or older must re-run the installer because the signed channel has been retired.

- [ ] **Step 4: Commit**

```bash
git add DEVELOPING.md RELEASE_SOP.md README.md
git commit -m "docs: describe the anchored release channel"
```

---

### Task 7: Release 0.1.6

**Files:**
- Modify: `Cargo.toml`, `Cargo.lock` (version bump to 0.1.6)
- Create: `releases/0.1.6/<target>.sha256` (anchor commit)

**Interfaces:**
- Consumes: everything from Tasks 1-6.
- Produces: the published `channel-<target>.json`, the immutable `0.1.6/<target>/` tree, the tag `v0.1.6`, and the anchor files in the repository.

- [ ] **Step 1: Bump the version**

Set `version = "0.1.6"` in `Cargo.toml`, then run `cargo check` so `Cargo.lock` matches.

- [ ] **Step 2: Run the local gate**

Run: `bash scripts/test.sh 2>&1 | tail -15`
Expected: format, clippy and the test suite pass, and `scripts/check_release.py` reports `version_changed=True`.

- [ ] **Step 3: Run the acceptance script and the secret scan**

Run: `python3 scripts/e2e_update.py 2>&1 | tail -5 && gitleaks git --redact --no-banner --log-opts='origin/main..HEAD'`
Expected: PASS line and no leaks.

- [ ] **Step 4: Commit and open the pull request**

```bash
git add -A && git commit -m "chore: bump release version to 0.1.6"
gh pr create --title "feat: replace the signed release channel with an anchored manifest" --body "<scope, trust model, evidence>"
```

- [ ] **Step 5: Merge through the release entry point**

Run: `bash scripts/land.sh`
Expected: the branch is pushed, the pull request is merged, and `origin/main` contains the merge commit.

- [ ] **Step 6: Build and package both targets**

```bash
cargo build --release --locked --target aarch64-apple-darwin
cargo zigbuild --release --locked --target x86_64-unknown-linux-gnu.2.30
python3 scripts/package.py --binary target/aarch64-apple-darwin/release/dytcn --target aarch64-apple-darwin --base-url https://data.doyoutrade.cloud/releases/dyt-ashare --out dist
python3 scripts/package.py --binary target/x86_64-unknown-linux-gnu/release/dytcn --target x86_64-unknown-linux-gnu --base-url https://data.doyoutrade.cloud/releases/dyt-ashare --out dist
```

- [ ] **Step 7: Commit the anchors and tag the release**

```bash
mkdir -p releases/0.1.6 && cp dist/releases/0.1.6/aarch64-apple-darwin.sha256 releases/0.1.6/ && cp dist/releases/0.1.6/x86_64-unknown-linux-gnu.sha256 releases/0.1.6/
git add releases/0.1.6 && git commit -m "chore: publish the 0.1.6 release channel anchors"
bash scripts/land.sh
git tag v0.1.6 && git push origin v0.1.6
```

- [ ] **Step 8: Publish the version tree, then the channel file**

Upload `dist/0.1.6/<target>/` to `/opt/ashare-data/cli-releases/0.1.6/<target>/`, then `dist/channel-<target>.json` to `/opt/ashare-data/cli-releases/channel-<target>.json`. Remove the retired `stable-<target>.json`.

- [ ] **Step 9: Acceptance**

In an isolated home: install from the published channel file, confirm the anchor check passes, confirm a tampered manifest is rejected, confirm a guest `coverage` call returns the `auth` block, and confirm `dytcn sync` without credentials is refused. Then reinstall the machine's existing 0.1.5 installation with the new installer, because the retired signed channel no longer updates it.

- [ ] **Step 10: Record the evidence**

Report: source commit, tag, both binary SHA-256 values, the manifest SHA-256 values, the anchor files, the acceptance results and the time.
