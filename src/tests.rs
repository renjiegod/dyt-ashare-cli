use super::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
#[test]
fn origin_credentials_paths_and_transport_are_restricted() {
    for s in [
        "http://example.com",
        "https://user:pass@example.com",
        "https://example.com/path",
        "https://example.com?token=secret",
    ] {
        assert!(state::origin(s).is_err());
    }
    assert!(state::origin("http://127.0.0.1:1234").is_ok());
    assert!(state::symbol("600000.SH").is_ok());
    assert!(state::symbol("../secret").is_err());
}

#[test]
fn api_load_without_credentials_uses_guest_mode() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(api::Api::load(tmp.path(), Some("http://127.0.0.1:8080")).is_ok());
}

#[test]
fn daily_range_splits_windows_and_merges_provenance_deterministically() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        for (date, request_index) in [("2024-01-02", 0), ("2025-01-02", 1)] {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 4096];
            let _ = stream.read(&mut request).unwrap();
            let body = json!({
                "items":[{"ts_code":"600000.SH","bar_end_time":format!("{date} 15:00:00"),"px_open":10.0,"px_high":10.0,"px_low":10.0,"px_close":10.0,"volume_shares":1000,"turnover_cny":10000}],
                "datasets":[{"id":"dataset-1","sha256":"sha-1","processor_version":"p1"}],
                "contract":{"version":"daily-from-minute-v1"},
                "coverage":"available_files_only",
                "order":"bar_end_time_desc",
                "request_index":request_index
            })
            .to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            )
            .unwrap();
        }
    });
    let api = api::Api::new(&format!("http://{}", address), None).unwrap();
    let value = api
        .daily_range("600000.SH", "2024-01-01", "2026-01-01")
        .unwrap();
    server.join().unwrap();
    assert_eq!(value["query"]["windows"].as_array().unwrap().len(), 2);
    assert_eq!(value["items"].as_array().unwrap().len(), 2);
    assert_eq!(value["items"][0]["bar_end_time"], "2025-01-02 15:00:00");
    assert_eq!(value["datasets"].as_array().unwrap().len(), 1);
}
#[test]
fn release_channel_rejects_tampering_wrong_target_bad_assets_and_path_escape() {
    let digest = "0".repeat(64);
    let asset = |path: &str, url: &str, sha: &str| json!({"path": path, "url": url, "bytes": 10, "sha256": sha});
    let base = update::Manifest {
        version: "0.1.6".into(),
        target: "x86_64-unknown-linux-gnu".into(),
        expires_at: None,
        files: vec![
            serde_json::from_value(asset(
                "dytcn",
                "https://data.doyoutrade.cloud/dytcn",
                &digest,
            ))
            .unwrap(),
            serde_json::from_value(asset(
                "skills/dyt-ashare/SKILL.md",
                "https://data.doyoutrade.cloud/skill.md",
                &digest,
            ))
            .unwrap(),
        ],
    };
    assert!(update::validate_manifest(&base, "x86_64-unknown-linux-gnu").is_ok());
    assert!(update::validate_manifest(&base, "aarch64-apple-darwin").is_err());
    let mut escape = base.clone();
    escape.files[0].path = "../../escape".into();
    assert!(update::validate_manifest(&escape, "x86_64-unknown-linux-gnu").is_err());
    let mut insecure = base.clone();
    insecure.files[0].url = "http://example.com/dytcn".into();
    assert!(update::validate_manifest(&insecure, "x86_64-unknown-linux-gnu").is_err());
    let mut broken = base.clone();
    broken.files[0].sha256 = "not-a-hash".into();
    assert!(update::validate_manifest(&broken, "x86_64-unknown-linux-gnu").is_err());
    let mut prerelease = base.clone();
    prerelease.version = "0.1.6-rc.1".into();
    assert!(update::validate_manifest(&prerelease, "x86_64-unknown-linux-gnu").is_err());
    let mut empty = base.clone();
    empty.files[0].bytes = 0;
    assert!(update::validate_manifest(&empty, "x86_64-unknown-linux-gnu").is_err());
}
#[test]
fn skill_update_preserves_user_edits_and_reports_actual_inventory() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("state");
    fs::create_dir(&root).unwrap();
    let dir = tmp.path().join("skills");
    skills::install(&root, &dir).unwrap();
    let p = dir.join("dyt-ashare/SKILL.md");
    fs::write(&p, b"my modified skill").unwrap();
    let result = skills::install(&root, &dir).unwrap();
    assert_eq!(result["pending"].as_array().unwrap().len(), 1);
    assert_eq!(fs::read(p).unwrap(), b"my modified skill");
    let inventory = skills::inventory(&root).unwrap();
    assert!(
        inventory
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["matches_installed_hash"] == false)
    );
}

#[test]
fn bundled_skill_is_one_broad_ashare_router() {
    assert_eq!(skills::BUNDLED.len(), 1);
    assert_eq!(skills::BUNDLED[0].0, "dyt-ashare");
    let text = skills::BUNDLED[0].1;
    for term in [
        "A股",
        "information",
        "analysis",
        "coverage",
        "minutes",
        "analyze",
    ] {
        assert!(
            text.to_ascii_lowercase()
                .contains(&term.to_ascii_lowercase()),
            "missing {term}"
        );
    }
}

#[test]
fn bundled_skill_guides_toward_an_api_key_without_forcing_one() {
    let text = skills::BUNDLED[0].1;
    for term in [
        "dytcn auth login",
        "dytcn auth status",
        "guest",
        "earliest_allowed_date",
        "Never ask the user to send an API key in chat",
        "work without login",
    ] {
        assert!(text.contains(term), "missing {term}");
    }
}

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
    assert!(
        update::check_anchor(
            raw,
            format!("{digest}  channel-x86_64-unknown-linux-gnu.json\n").as_bytes()
        )
        .is_ok()
    );
    assert!(
        update::check_anchor(
            raw,
            format!(
                "{}  channel-x86_64-unknown-linux-gnu.json\n",
                "0".repeat(64)
            )
            .as_bytes()
        )
        .is_err()
    );
    assert!(update::check_anchor(raw, b"").is_err());
    assert!(update::check_anchor(raw, b"not-a-digest\n").is_err());
}

#[test]
fn service_errors_map_every_limit_to_a_next_step() {
    for code in [
        "guest_minute_rate_limited",
        "guest_daily_rate_limited",
        "guest_rows_daily_limited",
        "guest_history_limit",
        "guest_access_disabled",
    ] {
        let error = api::ServiceError {
            status: 429,
            code: code.into(),
            retry_after_seconds: Some(60),
            authenticated: false,
        };
        assert!(error.hint().contains("dytcn auth login"), "{code}");
    }
    let account = api::ServiceError {
        status: 429,
        code: "account_daily_quota_exceeded".into(),
        retry_after_seconds: None,
        authenticated: true,
    };
    assert!(account.hint().contains("dytcn usage"));
    let rejected = api::ServiceError {
        status: 401,
        code: "invalid_api_key".into(),
        retry_after_seconds: None,
        authenticated: true,
    };
    assert!(rejected.hint().contains("dytcn auth login"));
}

#[test]
fn auth_status_reports_the_session_mode_and_the_login_next_step() {
    let tmp = tempfile::tempdir().unwrap();
    let guest = auth_status(tmp.path());
    assert_eq!(guest["stored_credentials"], false);
    if std::env::var_os("DYT_ASHARE_API_KEY").is_none()
        && std::env::var_os("DYT_CN_API_KEY").is_none()
    {
        assert_eq!(guest["mode"], "guest");
        assert_eq!(guest["guest_limits_apply"], true);
        assert!(
            guest["next_step"]
                .as_str()
                .unwrap()
                .contains("dytcn auth login")
        );
    }
    fs::write(tmp.path().join("credentials.json"), b"{}").unwrap();
    let account = auth_status(tmp.path());
    assert_eq!(account["mode"], "account");
    assert_eq!(account["guest_limits_apply"], false);
}

#[test]
fn data_answers_expose_the_session_mode_and_the_guest_notice() {
    let mut guest = json!({"items":[],"earliest_allowed_date":"2026-09-21"});
    annotate_auth(&mut guest, false);
    assert_eq!(guest["auth"]["mode"], "guest");
    assert_eq!(guest["auth"]["earliest_allowed_date"], "2026-09-21");
    assert!(
        guest["auth"]["notice"]
            .as_str()
            .unwrap()
            .contains("dytcn auth login")
    );
    let mut account = json!({"items":[]});
    annotate_auth(&mut account, true);
    assert_eq!(account["auth"]["mode"], "account");
    assert!(account["auth"].get("notice").is_none());
}

#[test]
fn detected_agent_destinations_include_universal_and_existing_agents() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join(".claude")).unwrap();
    fs::create_dir_all(tmp.path().join(".cursor")).unwrap();
    fs::create_dir_all(tmp.path().join(".workbuddy-ai")).unwrap();
    let destinations = skills::destinations_for_test(tmp.path());
    assert!(destinations.contains(&tmp.path().join(".agents/skills")));
    assert!(destinations.contains(&tmp.path().join(".claude/skills")));
    assert!(destinations.contains(&tmp.path().join(".cursor/skills")));
    assert!(destinations.contains(&tmp.path().join(".workbuddy-ai/skills")));
}

#[test]
fn workbuddy_destinations_require_an_existing_agent_root() {
    let tmp = tempfile::tempdir().unwrap();
    fs::create_dir_all(tmp.path().join(".workbuddy")).unwrap();
    let destinations = skills::destinations_for_test(tmp.path());
    assert!(destinations.contains(&tmp.path().join(".workbuddy/skills")));
    assert!(!destinations.contains(&tmp.path().join(".workbuddy-ai/skills")));
}

#[test]
fn workbuddy_skill_is_linked_next_to_the_canonical_skill() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let root = home.join("state");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(home.join(".workbuddy-ai")).unwrap();
    skills::install_for_test(&root, home).unwrap();
    let canonical = home.join(".agents/skills/dyt-ashare/SKILL.md");
    let linked = home.join(".workbuddy-ai/skills/dyt-ashare/SKILL.md");
    assert!(canonical.exists());
    assert!(linked.exists());
    assert_eq!(fs::read(&canonical).unwrap(), fs::read(&linked).unwrap());
}

#[cfg(unix)]
#[test]
fn skill_install_does_not_follow_a_foreign_skill_folder_symlink() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("state");
    let destination = tmp.path().join("skills");
    let outside = tmp.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::write(outside.join("SKILL.md"), b"foreign").unwrap();
    fs::create_dir_all(&destination).unwrap();
    std::os::unix::fs::symlink(&outside, destination.join("dyt-ashare")).unwrap();
    let result = skills::install(&root, &destination).unwrap();
    assert_eq!(
        result["pending"][0]["reason"],
        "foreign_or_symlinked_folder"
    );
    assert_eq!(fs::read(outside.join("SKILL.md")).unwrap(), b"foreign");
}

#[test]
fn unchanged_legacy_skills_are_removed_during_unified_install() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("state");
    let destination = tmp.path().join("skills");
    let legacy = destination.join("dyt-ashare-research/SKILL.md");
    fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    fs::write(&legacy, b"legacy").unwrap();
    fs::create_dir_all(&root).unwrap();
    let registry =
        json!({"entries":[{"path":legacy,"sha256":state::hash(b"legacy"),"version":"0.1.0"}]});
    fs::write(
        root.join("skills.json"),
        serde_json::to_vec(&registry).unwrap(),
    )
    .unwrap();
    let result = skills::install(&root, &destination).unwrap();
    assert_eq!(result["migrated"][0]["reason"], "legacy_skill_removed");
    assert!(!legacy.exists());
}
#[test]
fn local_analysis_is_chronological_and_exposes_quality_problems() {
    let bar = |t: &str, c: f64| json!({"ts_code":"600000.SH","bar_end_time":t,"px_open":c,"px_high":c,"px_low":c,"px_close":c,"volume_shares":1.0,"turnover_cny":1.0});
    let data = json!({"items":[bar("2026-06-01 09:32:00",8.0),bar("2026-06-01 09:31:00",10.0)]});
    let r = analysis::analyze(&serde_json::to_vec(&data).unwrap()).unwrap();
    assert!((r["raw_close_max_drawdown"].as_f64().unwrap() - 0.2).abs() < 1e-10);
    assert_eq!(r["first_bar"], "2026-06-01 09:31:00");
}
#[test]
fn updating_while_another_update_holds_lock_cannot_mutate_installation() {
    let tmp = tempfile::tempdir().unwrap();
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(tmp.path().join("update.lock"))
        .unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();
    assert!(update::run(tmp.path(), None, false).is_err());
    assert!(!tmp.path().join("active.json").exists());
}

#[test]
fn date_only_end_includes_the_whole_requested_day() {
    assert_eq!(
        minute_time("2026-06-01", true).unwrap(),
        "2026-06-01 23:59:59"
    );
    assert!(minute_time("2026-02-30", false).is_err());
}

#[test]
fn local_backtest_sorts_remote_daily_rows_and_writes_an_offline_report() {
    let bar = |date: &str, open: f64, close: f64| {
        json!({
            "ts_code":"600000.SH",
            "bar_end_time":format!("{date} 15:00:00"),
            "px_open":open,
            "px_high":open.max(close),
            "px_low":open.min(close),
            "px_close":close,
            "volume_shares":1000.0,
            "turnover_cny":open * 1000.0,
        })
    };
    let data = json!({
        "items":[
            bar("2026-01-03", 12.0, 13.0),
            bar("2026-01-02", 11.0, 12.0),
            bar("2026-01-01", 10.0, 11.0)
        ],
        "datasets":[{"id":"dataset-1","sha256":"abc","processor_version":"v1"}],
        "contract":{"version":"daily-from-minute-v1"},
        "query":{"symbol":"600000.SH","from":"2026-01-01","to":"2026-01-03","frequency":"1d"},
        "auth":{"mode":"guest"},
        "coverage":"available_files_only"
    });
    let config = backtest::Config {
        symbol: "600000.SH".into(),
        strategy: backtest::Strategy::parse("buy-and-hold", 10, 30).unwrap(),
        initial_cash: 100_000.0,
        fee_bps: 0.0,
        slippage_bps: 0.0,
        lot_size: 100,
        report_path: None,
    };
    let output = backtest::run(&data, &config).unwrap();
    assert_eq!(output.value["data"]["first_bar"], "2026-01-01 15:00:00");
    assert_eq!(output.value["data"]["last_bar"], "2026-01-03 15:00:00");
    assert_eq!(output.value["data"]["auth"]["mode"], "guest");
    assert_eq!(output.value["metrics"]["ending_equity"], 130_000.0);
    assert_eq!(output.value["metrics"]["terminal_state"]["flat"], true);
    assert!(output.html.contains("<svg"));
    assert!(!output.html.contains("https://"));
}

#[test]
fn local_backtest_rejects_duplicate_or_invalid_daily_rows() {
    let row = json!({
        "ts_code":"600000.SH",
        "bar_end_time":"2026-01-01 15:00:00",
        "px_open":10.0,
        "px_high":10.0,
        "px_low":10.0,
        "px_close":10.0,
        "volume_shares":1000.0,
        "turnover_cny":10000.0
    });
    let data = json!({"items":[row.clone(),row]});
    let config = backtest::Config {
        symbol: "600000.SH".into(),
        strategy: backtest::Strategy::BuyAndHold,
        initial_cash: 100_000.0,
        fee_bps: 0.0,
        slippage_bps: 0.0,
        lot_size: 100,
        report_path: None,
    };
    assert!(backtest::run(&data, &config).is_err());
}
