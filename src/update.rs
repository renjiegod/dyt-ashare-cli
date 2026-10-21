use crate::{api, skills, state};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
pub const TARGET: &str = env!("DYT_TARGET");
pub const ANCHOR_BASE: &str = "https://raw.githubusercontent.com/renjiegod/dyt-ashare-cli";
pub const RELEASE_BASE: &str = "https://data.doyoutrade.cloud/releases/dyt-ashare";

const MAX_FILE: u64 = 128 * 1024 * 1024;
const MAX_ANCHOR: u64 = 4096;
#[derive(Serialize, Deserialize, Clone)]
pub struct Manifest {
    pub version: String,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    pub files: Vec<Asset>,
}
#[derive(Serialize, Deserialize, Clone)]
pub struct Asset {
    pub path: String,
    pub url: String,
    pub bytes: u64,
    pub sha256: String,
}
#[derive(Serialize, Deserialize, Clone)]
pub struct Active {
    pub version: String,
    pub previous: Option<String>,
}
#[derive(Default, Serialize, Deserialize)]
struct Status {
    last_attempt: i64,
    last_success: Option<i64>,
    error: Option<String>,
    skills: Option<Value>,
}
pub fn binary() -> &'static str {
    if cfg!(windows) { "dytcn.exe" } else { "dytcn" }
}
/// The channel manifest is an unsigned, content-addressed description of a
/// release. It is trusted only after the repository anchor pins its bytes.
pub fn channel_manifest(raw: &[u8]) -> Result<Manifest> {
    serde_json::from_slice(raw).context("Invalid release channel manifest")
}

/// Installed releases written before 0.1.6 are legacy signed envelopes. Their
/// payload still describes the installed files, so integrity checks and
/// rollback keep working without a trust key.
pub fn installed_manifest(raw: &[u8]) -> Result<Manifest> {
    let value: Value = serde_json::from_slice(raw).context("Invalid installed release metadata")?;
    if let Some(payload) = value.get("payload").and_then(Value::as_str) {
        return serde_json::from_str(payload).context("Invalid legacy release payload");
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
    let text = String::from_utf8_lossy(anchor);
    let expected = text
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
    format!(
        "{}/v{version}/releases/{version}/{target}.sha256",
        anchor_base()
    )
}
fn verify_anchor(raw: &[u8], m: &Manifest, target: &str) -> Result<()> {
    let url = url::Url::parse(&anchor_url(&m.version, target))?;
    state::secure_url(&url)?;
    let anchor = api::bytes(
        api::http()?
            .get(url)
            .send()
            .context("Release anchor is unavailable")?,
        MAX_ANCHOR,
    )
    .map_err(|error| anyhow::anyhow!("Release anchor is unavailable: {error}"))?;
    check_anchor(raw, &anchor)
}
fn version_dir(root: &Path, version: &str) -> Result<PathBuf> {
    let v = semver::Version::parse(version)?;
    if v.to_string() != version || !v.pre.is_empty() {
        bail!("Invalid installed version");
    }
    Ok(root.join("versions").join(version))
}
pub fn active(root: &Path) -> Result<Option<Active>> {
    let p = root.join("active.json");
    if p.exists() {
        Ok(Some(state::read(&p)?))
    } else {
        Ok(None)
    }
}
fn smoke(path: &Path, version: &str) -> Result<()> {
    let out = bounded_command(
        Command::new(path)
            .arg("--version")
            .env("DYT_ASHARE_CHILD", "1")
            .env("DYT_ASHARE_NO_UPDATE", "1"),
    )?;
    if !out.status.success()
        || String::from_utf8_lossy(&out.stdout).trim() != format!("dytcn {version}")
    {
        bail!("New CLI failed version smoke test");
    }
    Ok(())
}
fn bounded_command(command: &mut Command) -> Result<std::process::Output> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("New CLI failed to start")?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if child.try_wait()?.is_some() {
            return Ok(child.wait_with_output()?);
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("New CLI startup timed out");
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}
fn verify_bundled_skills(exe: &Path, m: &Manifest) -> Result<()> {
    let out = bounded_command(
        Command::new(exe)
            .args(["skills", "bundled"])
            .env("DYT_ASHARE_CHILD", "1")
            .env("DYT_ASHARE_NO_UPDATE", "1"),
    )?;
    if !out.status.success() {
        bail!("New CLI bundled Skills check failed");
    }
    let value: Value = serde_json::from_slice(&out.stdout)?;
    let actual = value["skills"]
        .as_array()
        .context("Missing bundled Skills hashes")?;
    for f in m.files.iter().filter(|f| f.path.starts_with("skills/")) {
        if !actual
            .iter()
            .any(|s| s["path"] == f.path && s["sha256"] == f.sha256)
        {
            bail!("CLI and release Skills do not match");
        }
    }
    Ok(())
}
fn executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}
fn validate_installed(root: &Path, version: &str) -> Result<PathBuf> {
    let dir = version_dir(root, version)?;
    // Installed metadata is only read back for integrity; the channel trust
    // decision happened when the release was accepted.
    let raw = fs::read(dir.join("release.json"))?;
    let m = installed_manifest(&raw)?;
    if m.version != version {
        bail!("Installed release identity mismatch");
    }
    for f in &m.files {
        if !state::verified(&dir.join(&f.path), f.bytes, &f.sha256) {
            bail!("Installed release failed integrity check");
        }
    }
    Ok(dir)
}
fn apply(root: &Path, raw: &[u8]) -> Result<Value> {
    let m = channel_manifest(raw)?;
    validate_manifest(&m, TARGET)?;
    // The channel manifest is only trusted once the repository anchor pins
    // its exact bytes.
    verify_anchor(raw, &m, TARGET)?;
    let current = active(root)?;
    if let Some(a) = &current
        && semver::Version::parse(&m.version)? <= semver::Version::parse(&a.version)?
    {
        let refreshed = skills::refresh(root)
            .unwrap_or_else(|error| json!({"error":format!("skills_refresh_failed: {error}")}));
        return Ok(json!({"updated":false,"installed":a.version,"skills":refreshed}));
    }
    let versions = root.join("versions");
    fs::create_dir_all(&versions)?;
    let stage = tempfile::tempdir_in(&versions)?;
    let client = api::http()?;
    for f in &m.files {
        let data = api::bytes(
            client
                .get(&f.url)
                .send()
                .context("Release download failed")?,
            f.bytes,
        )
        .map_err(|error| anyhow::anyhow!("Release download failed: {error}"))?;
        if data.len() as u64 != f.bytes || state::hash(&data) != f.sha256 {
            bail!("Release asset checksum mismatch");
        }
        state::atomic(&stage.path().join(&f.path), &data)?;
    }
    let exe = stage.path().join(binary());
    executable(&exe)?;
    smoke(&exe, &m.version)?;
    verify_bundled_skills(&exe, &m)?;
    state::atomic(&stage.path().join("release.json"), raw)?;
    let dest = version_dir(root, &m.version)?;
    if dest.exists() {
        validate_installed(root, &m.version)?;
        if fs::read(dest.join("release.json"))? != raw {
            bail!("Refusing to rewrite an immutable installed version");
        }
    } else {
        fs::rename(stage.path(), &dest)?;
    }
    // The launcher is stable; running binaries stay in immutable version directories.
    let launcher = root.join("bin").join(binary());
    if !launcher.exists() {
        state::atomic(&launcher, &fs::read(dest.join(binary()))?)?;
        executable(&launcher)?;
    }
    let next = Active {
        version: m.version.clone(),
        previous: current.map(|a| a.version),
    };
    state::save(&root.join("active.json"), &next)?;
    // Ask the new version to refresh its bundled Skills. Preserve pending/error separately.
    let result = bounded_command(
        Command::new(dest.join(binary()))
            .args(["skills", "refresh"])
            .env("DYT_ASHARE_HOME", root)
            .env("DYT_ASHARE_CHILD", "1")
            .env("DYT_ASHARE_NO_UPDATE", "1"),
    );
    let skills = match result {
        Ok(out) if out.status.success() => serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|_| json!({"error":"invalid_skills_response"})),
        Ok(_) => json!({"error":"skills_refresh_failed","retry":"dytcn skills refresh"}),
        Err(_) => json!({"error":"skills_refresh_start_failed","retry":"dytcn skills refresh"}),
    };
    Ok(json!({"updated":true,"installed":next.version,"previous":next.previous,"skills":skills}))
}
pub fn run(root: &Path, source: Option<&Path>, background: bool) -> Result<Value> {
    if background
        && (root.join("updates-disabled").exists()
            || std::env::var_os("DYT_ASHARE_NO_UPDATE").is_some())
    {
        return Ok(json!({"updated":false,"reason":"disabled"}));
    }
    fs::create_dir_all(root)?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("update.lock"))?;
    fs2::FileExt::try_lock_exclusive(&lock).context("Another update is running")?;
    let now = chrono::Utc::now().timestamp();
    let status_path = root.join("update-status.json");
    let mut status = if status_path.exists() {
        state::read::<Status>(&status_path)?
    } else {
        Status::default()
    };
    let interval = if status.error.is_some() { 3600 } else { 21600 };
    if background && now - status.last_attempt < interval {
        return Ok(json!({"updated":false,"reason":"check_not_due"}));
    }
    status.last_attempt = now;
    state::save(&status_path, &status)?;
    let outcome = (|| {
        let raw = if let Some(p) = source {
            fs::read(p)?
        } else {
            let u = url::Url::parse(&channel_url(TARGET))?;
            state::secure_url(&u)?;
            api::bytes(
                api::http()?
                    .get(u)
                    .send()
                    .context("Release service unavailable")?,
                256 * 1024,
            )
            .map_err(|error| anyhow::anyhow!("Release service unavailable: {error}"))?
        };
        apply(root, &raw)
    })();
    match &outcome {
        Ok(value) => {
            status.last_success = Some(now);
            status.error = None;
            status.skills = value.get("skills").cloned();
        }
        Err(e) => {
            status.error = Some(format!("{e:#}"));
        }
    }
    state::save(&status_path, &status)?;
    outcome
}
pub fn rollback(root: &Path) -> Result<Value> {
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("update.lock"))?;
    fs2::FileExt::try_lock_exclusive(&lock)?;
    let a = active(root)?.context("No installed release")?;
    let previous = a.previous.context("No previous release")?;
    let dir = validate_installed(root, &previous)?;
    smoke(&dir.join(binary()), &previous)?;
    state::save(
        &root.join("active.json"),
        &Active {
            version: previous.clone(),
            previous: Some(a.version),
        },
    )?;
    // Pin after rollback so a startup check cannot immediately reapply the bad release.
    state::atomic(
        &root.join("updates-disabled"),
        b"rollback pin; run dytcn update enable to resume\n",
    )?;
    let out = Command::new(dir.join(binary()))
        .args(["skills", "refresh"])
        .env("DYT_ASHARE_HOME", root)
        .env("DYT_ASHARE_CHILD", "1")
        .env("DYT_ASHARE_NO_UPDATE", "1")
        .output()?;
    Ok(
        json!({"installed":previous,"auto_updates":false,"skills_refresh_succeeded":out.status.success()}),
    )
}
pub fn dispatch(root: &Path) -> Result<Option<i32>> {
    if std::env::var_os("DYT_ASHARE_CHILD").is_some() {
        return Ok(None);
    }
    let Some(a) = active(root)? else {
        return Ok(None);
    };
    let exe = version_dir(root, &a.version)?.join(binary());
    let current = std::env::current_exe()?;
    if fs::canonicalize(&exe)? == fs::canonicalize(&current)? {
        return Ok(None);
    }
    let code = Command::new(exe)
        .args(std::env::args_os().skip(1))
        .env("DYT_ASHARE_CHILD", "1")
        .status()?
        .code()
        .unwrap_or(1);
    Ok(Some(code))
}
pub fn background(root: &Path) {
    if std::env::var_os("DYT_ASHARE_NO_UPDATE").is_some()
        || root.join("updates-disabled").exists()
        || !root.join("active.json").exists()
    {
        return;
    }
    let mut c = Command::new(std::env::current_exe().unwrap_or_else(|_| PathBuf::from(binary())));
    c.args(["update", "--background"])
        .env("DYT_ASHARE_CHILD", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x08000000);
    }
    let _ = c.spawn();
}
pub fn status(root: &Path) -> Value {
    json!({"installed":active(root).ok().flatten(),"checks":state::read::<Value>(&root.join("update-status.json")).ok(),"automatic_updates_enabled":!root.join("updates-disabled").exists(),"target":TARGET,"channel":{"url":channel_url(TARGET),"anchor_base":anchor_base(),"verification":"sha256+anchor"}})
}
