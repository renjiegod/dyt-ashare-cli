mod analysis;
mod api;
mod backtest;
mod skills;
mod state;
mod update;
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read},
    path::PathBuf,
};
#[derive(Parser)]
#[command(
    name = "dytcn",
    version,
    about = "DYT AShare historical-market research tools"
)]
struct Cli {
    #[arg(long, global = true, env = "DYT_ASHARE_HOME")]
    home: Option<PathBuf>,
    #[arg(long, global = true, env = "DYT_ASHARE_API_URL")]
    api_url: Option<String>,
    #[arg(long, global = true)]
    out: Option<PathBuf>,
    #[arg(long, global = true, env = "DYT_ASHARE_NO_UPDATE", value_parser=clap::builder::BoolishValueParser::new())]
    no_update: bool,
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    /// Store a locally supplied key after server validation. Keys never enter command arguments.
    Auth {
        #[command(subcommand)]
        command: Auth,
    },
    /// Read server-enforced account entitlements and current usage.
    Usage,
    /// List available immutable stock/month versions, not a certified trading calendar.
    Coverage {
        #[arg(long)]
        symbol: String,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
    },
    /// Query a maximum of 31 calendar days; end is inclusive, Asia/Shanghai.
    Minutes {
        #[arg(long)]
        symbol: String,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
        #[arg(long, default_value = "1min")]
        freq: String,
    },
    /// Download and verify immutable Parquet and provenance, retaining old revisions.
    Sync {
        #[arg(long)]
        symbol: String,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Compute local descriptive statistics and quality checks from minutes JSON.
    Analyze {
        #[arg(long)]
        input: PathBuf,
    },
    /// Query daily bars from the remote AShare data API.
    Daily {
        #[arg(long)]
        symbol: String,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
    },
    /// Fetch daily bars and run a reproducible local strategy replay.
    Backtest {
        #[command(subcommand)]
        command: BacktestCommand,
    },
    Skills {
        #[command(subcommand)]
        command: Skills,
    },
    /// Verify, stage, smoke-test and activate a signed release bundle.
    Update {
        #[arg(long)]
        manifest: Option<PathBuf>,
        #[arg(long)]
        background: bool,
        #[command(subcommand)]
        action: Option<UpdateAction>,
    },
    /// Show actual installed versions, pending updates and local configuration.
    Doctor,
}
#[derive(Subcommand)]
enum Auth {
    Login {
        #[arg(long)]
        stdin: bool,
    },
    Logout,
    Status,
}
#[derive(Subcommand)]
enum Skills {
    Bundled,
    Install {
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    Refresh,
    List,
}
#[derive(Subcommand)]
enum BacktestCommand {
    /// Run a long-only daily-bar backtest and write a self-contained HTML report.
    Run {
        #[arg(long)]
        symbol: String,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
        #[arg(long, default_value = "sma-cross")]
        strategy: String,
        #[arg(long, default_value_t = 10)]
        fast: usize,
        #[arg(long, default_value_t = 30)]
        slow: usize,
        #[arg(long = "initial-cash", default_value_t = 100_000.0)]
        initial_cash: f64,
        #[arg(long, default_value_t = 3.0)]
        fee_bps: f64,
        #[arg(long, default_value_t = 2.0)]
        slippage_bps: f64,
        #[arg(long, default_value_t = 100)]
        lot_size: u64,
        #[arg(
            long,
            help = "HTML report path; defaults to dytcn-backtest-<symbol>-<strategy>.html"
        )]
        html: Option<PathBuf>,
    },
}
#[derive(Subcommand)]
enum UpdateAction {
    Enable,
    Disable,
    Rollback,
    Status,
}
fn run(cli: Cli, root: &std::path::Path) -> Result<Value> {
    let api = || api::Api::load(root, cli.api_url.as_deref());
    match cli.command {
        Commands::Auth { command } => match command {
            Auth::Login { stdin } => {
                let key = if stdin {
                    let mut k = String::new();
                    io::stdin().take(4096).read_to_string(&mut k)?;
                    k.trim().to_owned()
                } else {
                    rpassword::prompt_password("API key (hidden, stored only locally): ")?
                };
                api::login(
                    root,
                    cli.api_url
                        .as_deref()
                        .unwrap_or("https://data.doyoutrade.cloud"),
                    key,
                )
            }
            Auth::Logout => {
                let p = root.join("credentials.json");
                if p.exists() {
                    fs::remove_file(p)?;
                }
                Ok(
                    json!({"logged_out":true,"environment_key_set":std::env::var_os("DYT_ASHARE_API_KEY").or_else(|| std::env::var_os("DYT_CN_API_KEY")).is_some()}),
                )
            }
            Auth::Status => Ok(auth_status(root)),
        },
        Commands::Usage => api()?.get("/v1/usage", &[]),
        Commands::Coverage { symbol, from, to } => {
            state::symbol(&symbol)?;
            let api = api()?;
            let mut v = api.get(
                "/v1/datasets",
                &[("ts_code", &symbol), ("from", &from), ("to", &to)],
            )?;
            annotate_auth(&mut v, api.authenticated());
            Ok(v)
        }
        Commands::Minutes {
            symbol,
            from,
            to,
            freq,
        } => {
            state::symbol(&symbol)?;
            let from = minute_time(&from, false)?;
            let to = minute_time(&to, true)?;
            let api = api()?;
            let mut v = api.get(
                "/v1/minutes",
                &[
                    ("ts_code", &symbol),
                    ("start_date", &from),
                    ("end_date", &to),
                    ("freq", &freq),
                ],
            )?;
            v["cli_version"] = json!(env!("CARGO_PKG_VERSION"));
            v["query"] = json!({"symbol":symbol,"from":from,"to":to,"frequency":freq});
            annotate_auth(&mut v, api.authenticated());
            Ok(v)
        }
        Commands::Sync {
            symbol,
            from,
            to,
            dir,
        } => api()?.sync(
            &symbol,
            &from,
            &to,
            &dir.unwrap_or_else(|| api::default_data(root)),
        ),
        Commands::Analyze { input } => {
            analysis::analyze(&fs::read(input).context("Cannot read analysis input")?)
        }
        Commands::Daily { symbol, from, to } => {
            let api = api()?;
            let mut value = api.daily_range(&symbol, &from, &to)?;
            annotate_auth(&mut value, api.authenticated());
            Ok(value)
        }
        Commands::Backtest { command } => match command {
            BacktestCommand::Run {
                symbol,
                from,
                to,
                strategy,
                fast,
                slow,
                initial_cash,
                fee_bps,
                slippage_bps,
                lot_size,
                html,
            } => {
                state::symbol(&symbol)?;
                let api = api()?;
                let mut data = api.daily_range(&symbol, &from, &to)?;
                annotate_auth(&mut data, api.authenticated());
                let config = backtest::Config {
                    symbol,
                    strategy: backtest::Strategy::parse(&strategy, fast, slow)?,
                    initial_cash,
                    fee_bps,
                    slippage_bps,
                    lot_size,
                    report_path: html,
                };
                let output = backtest::run(&data, &config)?;
                let report_path = backtest::default_report_path(&config);
                state::atomic(&report_path, output.html.as_bytes())?;
                let report_path = fs::canonicalize(&report_path)?;
                let report_bytes = fs::read(&report_path)?;
                let mut value = output.value;
                value["report"] = json!({
                    "html_path": report_path,
                    "bytes": report_bytes.len(),
                    "sha256": state::hash(&report_bytes),
                    "self_contained": true,
                });
                Ok(value)
            }
        },
        Commands::Skills { command } => match command {
            Skills::Bundled => Ok(
                json!({"version":env!("CARGO_PKG_VERSION"),"skills":skills::BUNDLED.iter().map(|(name,text)|json!({"path":format!("skills/{name}/SKILL.md"),"sha256":state::hash(text.as_bytes())})).collect::<Vec<_>>()}),
            ),
            Skills::Install { dir } => match dir {
                Some(dir) => skills::install(root, &dir),
                None => skills::install_default(root),
            },
            Skills::Refresh => skills::refresh(root),
            Skills::List => skills::inventory(root),
        },
        Commands::Update {
            manifest,
            background,
            action,
        } => match action {
            Some(UpdateAction::Status) => Ok(update::status(root)),
            Some(UpdateAction::Disable) => {
                state::atomic(&root.join("updates-disabled"), b"disabled by user\n")?;
                Ok(json!({"automatic_updates_enabled":false}))
            }
            Some(UpdateAction::Enable) => {
                let p = root.join("updates-disabled");
                if p.exists() {
                    fs::remove_file(p)?;
                }
                Ok(json!({"automatic_updates_enabled":true}))
            }
            Some(UpdateAction::Rollback) => update::rollback(root),
            None => update::run(root, manifest.as_deref(), background),
        },
        Commands::Doctor => Ok(
            json!({"cli_version":env!("CARGO_PKG_VERSION"),"executable":std::env::current_exe()?,"home":root,"service_origin":cli.api_url.unwrap_or_else(||"https://data.doyoutrade.cloud".into()),"updates":update::status(root),"skills":skills::inventory(root)?,"skills_status":state::read::<Value>(&root.join("skills-status.json")).ok(),"credentials_stored":root.join("credentials.json").exists(),"auth":auth_status(root)}),
        ),
    }
}
/// The session mode is a fact the caller needs: guest queries work, but the
/// server caps them. The mode is reported, never enforced locally.
fn auth_status(root: &std::path::Path) -> Value {
    let stored = root.join("credentials.json").exists();
    let environment = std::env::var_os("DYT_ASHARE_API_KEY")
        .or_else(|| std::env::var_os("DYT_CN_API_KEY"))
        .is_some();
    let mode = if stored || environment {
        "account"
    } else {
        "guest"
    };
    json!({
        "mode": mode,
        "stored_credentials": stored,
        "environment_key_set": environment,
        "guest_limits_apply": mode == "guest",
        "next_step": if mode == "guest" {
            "Guest access works without a key. The server limits guest request rate, daily requests and rows, and the history window. Run `dytcn auth login` for account quota and full history."
        } else {
            "Authenticated session. `dytcn usage` reports the account quota and `dytcn sync` is available."
        },
    })
}

/// Attach the session mode to a successful response so a guest answer explains
/// its own limits without the caller having to ask for them.
fn annotate_auth(value: &mut Value, authenticated: bool) {
    let earliest = value
        .get("earliest_allowed_date")
        .and_then(Value::as_str)
        .map(str::to_owned);
    let mut block = json!({"mode": if authenticated { "account" } else { "guest" }});
    if let Some(date) = earliest {
        block["earliest_allowed_date"] = json!(date);
    }
    if !authenticated {
        block["notice"] = json!(api::GUEST_NOTICE);
    }
    value["auth"] = block;
}
fn minute_time(raw: &str, end: bool) -> Result<String> {
    if raw.len() == 10 {
        chrono::NaiveDate::parse_from_str(raw, "%Y-%m-%d").context("Expected YYYY-MM-DD")?;
        Ok(format!(
            "{raw} {}",
            if end { "23:59:59" } else { "00:00:00" }
        ))
    } else {
        chrono::NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S")
            .context("Expected a date or Shanghai timestamp")?;
        Ok(raw.into())
    }
}
fn main() {
    let result = (|| -> Result<()> {
        let root = state::root()?;
        if let Some(code) = update::dispatch(&root)? {
            std::process::exit(code);
        }
        let cli = Cli::parse();
        fs::create_dir_all(&root)?;
        if !cli.no_update && !matches!(cli.command, Commands::Update { .. }) {
            update::background(&root);
        }
        let out = cli.out.clone();
        let value = run(cli, &root)?;
        if let Some(path) = out {
            let data = serde_json::to_vec_pretty(&value)?;
            state::atomic(&path, &data)?;
            println!(
                "{}",
                json!({"output_path":fs::canonicalize(path)?,"bytes":data.len(),"sha256":state::hash(&data)})
            );
        } else {
            println!("{}", serde_json::to_string(&value)?);
        }
        Ok(())
    })();
    if let Err(e) = result {
        let payload = match e.downcast_ref::<api::ServiceError>() {
            Some(service) => json!({
                "error": service.to_string(),
                "status": service.status,
                "code": service.code,
                "retry_after_seconds": service.retry_after_seconds,
                "authenticated": service.authenticated,
                "hint": service.hint(),
            }),
            None => json!({"error": format!("{e:#}")}),
        };
        eprintln!("{payload}");
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests;
