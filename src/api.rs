use crate::state;
use anyhow::{Context, Result, bail};
use chrono::{Duration as CalendarDuration, NaiveDate};
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
/// Guest sessions are server-limited, not client-limited. Every guest answer
/// carries this notice so the caller can offer an API key without blocking the
/// request that already succeeded.
pub const GUEST_NOTICE: &str = "Guest session: the server enforces a request rate limit, a daily request and row quota, and a limited history window. Run `dytcn auth login` for account quota and full history.";
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
/// A server refusal the caller can act on: the service code, the HTTP status,
/// the retry window and one concrete next step. Guest limits are reported, not
/// enforced locally.
#[derive(Debug, Clone)]
pub struct ServiceError {
    pub status: u16,
    pub code: String,
    pub retry_after_seconds: Option<u64>,
    pub authenticated: bool,
}
impl ServiceError {
    pub fn authenticated(mut self, authenticated: bool) -> Self {
        self.authenticated = authenticated;
        self
    }
    pub fn hint(&self) -> &'static str {
        match self.code.as_str() {
            "guest_minute_rate_limited" => {
                "Guest request rate limit hit. Wait for the refill or `dytcn auth login`."
            }
            "guest_daily_rate_limited" => {
                "Guest daily request quota is exhausted. Run `dytcn auth login`."
            }
            "guest_rows_daily_limited" => {
                "Guest daily row quota is exhausted. Narrow the range or `dytcn auth login`."
            }
            "guest_history_limit" => {
                "Range starts before the guest history window. Narrow it or `dytcn auth login`."
            }
            "guest_access_disabled" => {
                "Guest access is disabled. Run `dytcn auth login` to use an account key."
            }
            "guest_limiter_unavailable" | "guest_policy_unavailable" => {
                "Server limiter unavailable. Retry later; a key does not change this."
            }
            "invalid_api_key" => "The service rejected the key. Run `dytcn auth login` again.",
            "account_daily_quota_exceeded" => {
                "Account daily request quota is exhausted. See `dytcn usage`."
            }
            "account_minute_quota_exceeded" => {
                "Account per-minute quota is exhausted. Slow down and retry."
            }
            "account_concurrency_limit" | "minute_concurrency_limit" => {
                "Account concurrency limit reached. Send fewer parallel requests."
            }
            "account_symbol_limit" => "Account symbol quota is exhausted. See `dytcn usage`.",
            "account_data_quota_exceeded" => "Account data quota is exhausted. See `dytcn usage`.",
            "history_plan_limit" => {
                "The account plan does not cover this history range. Ask the operator."
            }
            _ => "Check `dytcn doctor`, `dytcn update status` and `dytcn usage`.",
        }
    }
}
impl std::fmt::Display for ServiceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "HTTP {}: {}", self.status, self.code)
    }
}
impl std::error::Error for ServiceError {}
pub fn http() -> Result<Client> {
    Ok(Client::builder()
        .redirect(Policy::none())
        .timeout(Duration::from_secs(60))
        .user_agent(concat!("dyt-ashare/", env!("CARGO_PKG_VERSION")))
        .build()?)
}
pub fn bytes(mut r: Response, limit: u64) -> Result<Vec<u8>, ServiceError> {
    let status = r.status();
    if !status.is_success() {
        let retry = r
            .headers()
            .get("retry-after")
            .and_then(|s| s.to_str().ok())
            .and_then(|s| s.trim().parse::<u64>().ok());
        let mut body = String::new();
        let _ = r.take(8192).read_to_string(&mut body);
        let code = serde_json::from_str::<Value>(&body)
            .ok()
            .and_then(|v| v["error"].as_str().map(str::to_owned))
            .unwrap_or_else(|| "request_failed".into());
        return Err(ServiceError {
            status: status.as_u16(),
            code,
            retry_after_seconds: retry,
            authenticated: false,
        });
    }
    if r.content_length().is_some_and(|n| n > limit) {
        return Err(ServiceError {
            status: status.as_u16(),
            code: "response_too_large".into(),
            retry_after_seconds: None,
            authenticated: false,
        });
    }
    let mut data = Vec::new();
    r.by_ref()
        .take(limit + 1)
        .read_to_end(&mut data)
        .map_err(|error| ServiceError {
            status: status.as_u16(),
            code: format!("response_read_failed: {error}"),
            retry_after_seconds: None,
            authenticated: false,
        })?;
    if data.len() as u64 > limit {
        return Err(ServiceError {
            status: status.as_u16(),
            code: "response_too_large".into(),
            retry_after_seconds: None,
            authenticated: false,
        });
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
    pub fn authenticated(&self) -> bool {
        self.key.is_some()
    }
    pub fn get(&self, path: &str, query: &[(&str, &str)]) -> Result<Value> {
        Ok(serde_json::from_slice(&self.raw(path, query)?)?)
    }

    pub fn daily(&self, symbol: &str, from: &str, to: &str) -> Result<Value> {
        state::symbol(symbol)?;
        let from_date = chrono::NaiveDate::parse_from_str(from, "%Y-%m-%d")
            .context("Expected --from as YYYY-MM-DD")?;
        let to_date = chrono::NaiveDate::parse_from_str(to, "%Y-%m-%d")
            .context("Expected --to as YYYY-MM-DD")?;
        if to_date < from_date {
            bail!("--to must be on or after --from");
        }
        if (to_date - from_date).num_days() >= 366 {
            bail!("Daily API requests are limited to 366 calendar days");
        }
        let from_text = from_date.format("%Y-%m-%d").to_string();
        let to_text = to_date.format("%Y-%m-%d").to_string();
        let mut value = self.get(
            "/v1/daily",
            &[
                ("ts_code", symbol),
                ("start_date", from_text.as_str()),
                ("end_date", to_text.as_str()),
            ],
        )?;
        value["cli_version"] = json!(env!("CARGO_PKG_VERSION"));
        value["query"] = json!({"symbol":symbol,"from":from_text,"to":to_text,"frequency":"1d"});
        Ok(value)
    }

    /// Fetch a long daily window as bounded sequential API requests. Each
    /// request remains within the service's 366-calendar-day contract and the
    /// merged response keeps every query window and dataset identity.
    pub fn daily_range(&self, symbol: &str, from: &str, to: &str) -> Result<Value> {
        state::symbol(symbol)?;
        let from_date =
            NaiveDate::parse_from_str(from, "%Y-%m-%d").context("Expected --from as YYYY-MM-DD")?;
        let to_date =
            NaiveDate::parse_from_str(to, "%Y-%m-%d").context("Expected --to as YYYY-MM-DD")?;
        if to_date < from_date {
            bail!("--to must be on or after --from");
        }
        let mut cursor = from_date;
        let mut items = Vec::new();
        let mut datasets = Vec::new();
        let mut windows = Vec::new();
        let mut contract = Value::Null;
        let mut coverage = Value::Null;
        while cursor <= to_date {
            let chunk_end = (cursor + CalendarDuration::days(365)).min(to_date);
            let chunk_from = cursor.format("%Y-%m-%d").to_string();
            let chunk_to = chunk_end.format("%Y-%m-%d").to_string();
            let response = self.daily(symbol, &chunk_from, &chunk_to)?;
            if contract.is_null() {
                contract = response["contract"].clone();
            } else if response["contract"] != contract {
                bail!("Daily API contract changed between query windows");
            }
            if coverage.is_null() {
                coverage = response["coverage"].clone();
            }
            if let Some(rows) = response["items"].as_array() {
                items.extend(rows.iter().cloned());
            }
            if let Some(objects) = response["datasets"].as_array() {
                for object in objects {
                    if let Some(existing) = datasets
                        .iter()
                        .find(|existing: &&Value| existing["id"] == object["id"])
                        && existing["sha256"] != object["sha256"]
                    {
                        bail!("Dataset revision changed between query windows");
                    }
                    let duplicate = datasets.iter().any(|existing: &Value| {
                        existing["id"] == object["id"] && existing["sha256"] == object["sha256"]
                    });
                    if !duplicate {
                        datasets.push(object.clone());
                    }
                }
            }
            windows.push(response["query"].clone());
            cursor = chunk_end + CalendarDuration::days(1);
        }
        items.sort_by(|left, right| {
            right["bar_end_time"]
                .as_str()
                .cmp(&left["bar_end_time"].as_str())
        });
        Ok(json!({
            "items": items,
            "datasets": datasets,
            "contract": contract,
            "coverage": coverage,
            "order": "bar_end_time_desc",
            "cli_version": env!("CARGO_PKG_VERSION"),
            "query": {"symbol":symbol,"from":from_date.to_string(),"to":to_date.to_string(),"frequency":"1d","windows":windows},
        }))
    }
    pub fn raw(&self, path: &str, query: &[(&str, &str)]) -> Result<Vec<u8>> {
        let mut request = self.client.get(self.origin.join(path)?).query(query);
        if let Some(key) = &self.key {
            request = request.bearer_auth(key);
        }
        let response = request.send().context("Service request failed")?;
        Ok(bytes(response, LIMIT).map_err(|error| error.authenticated(self.authenticated()))?)
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
        let response = req.send().context("Dataset download failed")?;
        Ok(bytes(response, LIMIT).map_err(|error| error.authenticated(self.authenticated()))?)
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
