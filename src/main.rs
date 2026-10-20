mod analysis;
mod api;
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
    name = "dyt-cn",
    version,
    about = "DYT CN historical-market research tools"
)]
struct Cli {
    #[arg(long, global = true, env = "DYT_CN_HOME")]
    home: Option<PathBuf>,
    #[arg(long, global = true, env = "DYT_CN_API_URL")]
    api_url: Option<String>,
    #[arg(long, global = true)]
    out: Option<PathBuf>,
    #[arg(long, global = true, env = "DYT_CN_NO_UPDATE", value_parser=clap::builder::BoolishValueParser::new())]
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
    Install {
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    Refresh,
    List,
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
                    json!({"logged_out":true,"environment_key_set":std::env::var_os("DYT_CN_API_KEY").is_some()}),
                )
            }
            Auth::Status => Ok(
                json!({"stored_credentials":root.join("credentials.json").exists(),"environment_key_set":std::env::var_os("DYT_CN_API_KEY").is_some()}),
            ),
        },
        Commands::Usage => api()?.get("/v1/usage", &[]),
        Commands::Coverage { symbol, from, to } => {
            state::symbol(&symbol)?;
            api()?.get(
                "/v1/datasets",
                &[("ts_code", &symbol), ("from", &from), ("to", &to)],
            )
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
            let mut v = api()?.get(
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
        Commands::Skills { command } => match command {
            Skills::Install { dir } => {
                skills::install(root, &dir.map(Ok).unwrap_or_else(skills::default_dir)?)
            }
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
            json!({"cli_version":env!("CARGO_PKG_VERSION"),"executable":std::env::current_exe()?,"home":root,"service_origin":cli.api_url.unwrap_or_else(||"https://data.doyoutrade.cloud".into()),"updates":update::status(root),"skills":skills::inventory(root)?,"skills_status":state::read::<Value>(&root.join("skills-status.json")).ok(),"credentials_stored":root.join("credentials.json").exists()}),
        ),
    }
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
        eprintln!("{}", json!({"error":format!("{e:#}")}));
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests;
