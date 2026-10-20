use anyhow::{Context, Result, bail};
use serde::{Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
pub fn home() -> Result<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .context("Cannot resolve user home")
}
pub fn root() -> Result<PathBuf> {
    let args = std::env::args_os().collect::<Vec<_>>();
    for (i, arg) in args.iter().enumerate().skip(1) {
        if arg == "--home" {
            return args
                .get(i + 1)
                .map(PathBuf::from)
                .context("--home needs a path");
        }
        if let Some(s) = arg.to_str().and_then(|s| s.strip_prefix("--home=")) {
            return Ok(PathBuf::from(s));
        }
    }
    if let Some(p) = std::env::var_os("DYT_ASHARE_HOME").or_else(|| std::env::var_os("DYT_CN_HOME"))
    {
        return Ok(PathBuf::from(p));
    }
    #[cfg(windows)]
    {
        Ok(
            PathBuf::from(std::env::var_os("LOCALAPPDATA").context("LOCALAPPDATA is missing")?)
                .join("DYT/AShare"),
        )
    }
    #[cfg(not(windows))]
    {
        Ok(home()?.join(".local/share/dyt-ashare"))
    }
}
pub fn read<T: DeserializeOwned>(path: &Path) -> Result<T> {
    serde_json::from_slice(
        &fs::read(path).with_context(|| format!("Cannot read {}", path.display()))?,
    )
    .context("Invalid local JSON")
}
pub fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("Path has no parent")?;
    fs::create_dir_all(parent)?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}
pub fn save(path: &Path, value: &impl Serialize) -> Result<()> {
    atomic(path, &serde_json::to_vec_pretty(value)?)
}
pub fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
pub fn verified(path: &Path, size: u64, sha: &str) -> bool {
    fs::read(path).is_ok_and(|v| v.len() as u64 == size && hash(&v) == sha)
}
pub fn symbol(s: &str) -> Result<()> {
    if s.len() != 9
        || !s.as_bytes()[..6].iter().all(u8::is_ascii_digit)
        || ![".SH", ".SZ", ".BJ"].contains(&&s[6..])
    {
        bail!("Use a six-digit symbol with .SH, .SZ or .BJ exchange suffix");
    }
    Ok(())
}
pub fn origin(raw: &str) -> Result<url::Url> {
    let u = url::Url::parse(raw).context("Invalid service origin")?;
    if !u.username().is_empty()
        || u.password().is_some()
        || u.query().is_some()
        || u.fragment().is_some()
        || u.path() != "/"
    {
        bail!("Service URL must be an origin without credentials, path or query");
    }
    secure_url(&u)?;
    Ok(u)
}
pub fn secure_url(u: &url::Url) -> Result<()> {
    let local = matches!(u.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if u.scheme() != "https" && !(u.scheme() == "http" && local) {
        bail!("HTTPS is required outside loopback");
    }
    if !u.username().is_empty() || u.password().is_some() {
        bail!("URL credentials are forbidden");
    }
    Ok(())
}
