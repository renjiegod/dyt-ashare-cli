use crate::state;
use anyhow::{Context, Result, bail};
use reqwest::{
    blocking::{Client, Response},
    redirect::Policy,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};
pub const LIMIT: u64 = 8 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
pub struct Credentials {
    pub origin: String,
    pub key: String,
}
pub struct Api {
    client: Client,
    origin: url::Url,
    key: Option<String>,
}
pub fn http() -> Result<Client> {
    Ok(Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(60))
        .user_agent(concat!("dyt-ashare/", env!("CARGO_PKG_VERSION")))
        .build()?)
}
pub fn bytes(mut r: Response, limit: u64) -> Result<Vec<u8>> {
    if !r.status().is_success() {
        let status = r.status();
        let retry = r
            .headers()
            .get("retry-after")
            .and_then(|s| s.to_str().ok())
            .unwrap_or("unspecified")
            .to_owned();
        let mut body = String::new();
        r.take(8192).read_to_string(&mut body)?;
        let code = serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|v| v["error"].as_str().map(str::to_owned))
            .unwrap_or_else(|| "request_failed".into());
        bail!("HTTP {status}: {code}; retry_after_seconds={retry}");
    }
    if r.content_length().is_some_and(|n| n > limit) {
        bail!("Response exceeds size limit");
    }
    let mut data = Vec::new();
    r.by_ref().take(limit + 1).read_to_end(&mut data)?;
    if data.len() as u64 > limit {
        bail!("Response exceeds size limit");
    }
    Ok(data)
}
impl Api {
    pub fn new(origin: &str, key: Option<String>) -> Result<Self> {
        Ok(Self {
            client: http()?,
            origin: state::origin(origin)?,
            key,
        })
    }
    pub fn load(root: &Path, explicit: Option<&str>) -> Result<Self> {
        let creds = if root.join("credentials.json").exists() {
            Some(state::read::<Credentials>(&root.join("credentials.json"))?)
        } else {
            None
        };
        let origin = explicit
            .map(str::to_owned)
            .or_else(|| creds.as_ref().map(|c| c.origin.clone()))
            .unwrap_or_else(|| "https://data.doyoutrade.cloud".into());
        if std::env::var_os("DYT_ASHARE_API_KEY")
            .or_else(|| std::env::var_os("DYT_CN_API_KEY"))
            .is_none()
            && creds
                .as_ref()
                .is_some_and(|c| c.origin.trim_end_matches('/') != origin.trim_end_matches('/'))
        {
            bail!(
                "Stored credentials belong to a different origin; log in to the selected service first"
            );
        }
        let key = std::env::var("DYT_ASHARE_API_KEY")
            .ok()
            .or_else(|| std::env::var("DYT_CN_API_KEY").ok())
            .or_else(|| creds.map(|c| c.key));
        Self::new(&origin, key)
    }
    pub fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value> {
        Ok(serde_json::from_slice(&self.raw(path, query)?)?)
    }
    pub fn raw(&self, path: &str, query: &[(&str, &str)]) -> Result<Vec<u8>> {
        let mut request = self.client.get(self.origin.join(path)?).query(query);
        if let Some(key) = &self.key {
            request = request.bearer_auth(key);
        }
        bytes(request.send().context("Service request failed")?, LIMIT)
    }
    fn download(&self, raw: &str) -> Result<Vec<u8>> {
        let u = self.origin.join(raw)?;
        state::secure_url(&u)?;
        let mut req = self.client.get(u.clone());
        // API credentials must never reach object storage or redirected URLs.
        if u.origin() == self.origin.origin()
            && let Some(key) = &self.key
        {
            req = req.bearer_auth(key);
        }
        bytes(req.send().context("Dataset download failed")?, LIMIT)
    }
    pub fn sync(&self, symbol: &str, from: &str, to: &str, dir: &Path) -> Result<Value> {
        self.require_key("dytcn sync")?;
        state::symbol(symbol)?;
        let mut catalog = self.get(
            "/v1/datasets",
            &[("ts_code", symbol), ("from", from), ("to", to)],
        )?;
        let objects = catalog["items"]
            .as_array_mut()
            .context("Catalog missing items")?;
        let mut downloaded = 0;
        let mut skipped = 0;
        let base = dir.join(symbol);
        fs::create_dir_all(&base)?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(base.join("sync.lock"))?;
        fs2::FileExt::try_lock_exclusive(&lock)
            .context("Another sync is running for this symbol")?;
        for o in objects {
            let id = o["id"].as_str().context("Dataset ID missing")?;
            if id.len() != 36 || !id.bytes().all(|c| c.is_ascii_hexdigit() || c == b'-') {
                bail!("Invalid dataset ID");
            }
            let sha = o["sha256"].as_str().context("Dataset hash missing")?;
            let size = o["bytes"]
                .as_u64()
                .filter(|n| *n <= LIMIT)
                .context("Invalid dataset size")?;
            let path = base.join(format!("{id}.parquet"));
            let manifest_path = base.join(format!("{id}.manifest.json"));
            let manifest = self.raw(&format!("/v1/datasets/{id}/manifest"), &[])?;
            if state::hash(&manifest)
                != o["manifest_sha256"]
                    .as_str()
                    .context("Manifest hash missing")?
            {
                bail!("Manifest checksum mismatch");
            }
            if state::verified(&path, size, sha) {
                skipped += 1;
            } else {
                let ticket = self.get(&format!("/v1/datasets/{id}/download"), &[])?;
                if ticket["dataset"]["sha256"] != o["sha256"] {
                    bail!("Download ticket changed immutable identity");
                }
                let data =
                    self.download(ticket["url"].as_str().context("Download URL missing")?)?;
                if data.len() as u64 != size || state::hash(&data) != sha {
                    bail!("Dataset size or SHA256 mismatch");
                }
                state::atomic(&path, &data)?;
                downloaded += 1;
            }
            state::atomic(&manifest_path, &manifest)?;
            o["local_path"] = json!(fs::canonicalize(&path)?);
            o["local_manifest_path"] = json!(fs::canonicalize(&manifest_path)?);
        }
        catalog["query"] = json!({"symbol":symbol,"from":from,"to":to});
        catalog["cli_version"] = json!(env!("CARGO_PKG_VERSION"));
        let path = base.join("catalog.json");
        state::save(&path, &catalog)?;
        Ok(
            json!({"catalog_path":fs::canonicalize(path)?,"catalog_sha256":state::hash(&serde_json::to_vec_pretty(&catalog)?),"downloaded":downloaded,"verified_skipped":skipped,"restricted_downloads":catalog["restricted_downloads"],"cli_version":env!("CARGO_PKG_VERSION")}),
        )
    }

    fn require_key(&self, command: &str) -> Result<()> {
        if self.key.is_none() {
            bail!("{command} requires login; run dytcn auth login first");
        }
        Ok(())
    }
}
pub fn login(root: &Path, origin: &str, key: String) -> Result<Value> {
    if !key.starts_with("ask_")
        || key.len() != 68
        || !key[4..].bytes().all(|b| b.is_ascii_hexdigit())
    {
        bail!("Invalid API key format");
    }
    let api = Api::new(origin, Some(key.clone()))?;
    let usage = api.get("/v1/usage", &[])?;
    state::save(
        &root.join("credentials.json"),
        &Credentials {
            origin: origin.into(),
            key,
        },
    )?;
    Ok(json!({"authenticated":true,"usage":usage}))
}
pub fn default_data(root: &Path) -> PathBuf {
    root.join("datasets")
}
