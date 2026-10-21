use super::*;
use ed25519_dalek::{Signer, SigningKey};
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
fn release() -> update::Manifest {
    update::Manifest {
        version: "0.1.0".into(),
        target: update::TARGET.into(),
        expires_at: 200,
        files: [update::binary(), "skills/dyt-ashare/SKILL.md"]
            .iter()
            .map(|p| update::Asset {
                path: (*p).into(),
                url: "https://example.com/asset".into(),
                bytes: 10,
                sha256: "0".repeat(64),
            })
            .collect(),
    }
}
fn signed(m: &update::Manifest, key: &SigningKey) -> Vec<u8> {
    let payload = serde_json::to_string(m).unwrap();
    let signature = hex::encode(key.sign(payload.as_bytes()).to_bytes());
    serde_json::to_vec(&update::Envelope { payload, signature }).unwrap()
}
#[test]
fn release_trust_checks_reject_tampering_wrong_target_expiry_and_traversal() {
    let key = SigningKey::from_bytes(&[7; 32]);
    let public = key.verifying_key().to_bytes();
    let mut m = release();
    assert!(update::verify(&signed(&m, &key), &public, update::TARGET, 100).is_ok());
    assert!(update::verify(&signed(&m, &key), &public, "wrong", 100).is_err());
    assert!(update::verify(&signed(&m, &key), &public, update::TARGET, 201).is_err());
    assert!(
        update::verify(
            &signed(&m, &key),
            &SigningKey::from_bytes(&[8; 32]).verifying_key().to_bytes(),
            update::TARGET,
            100
        )
        .is_err()
    );
    let mut raw: Value = serde_json::from_slice(&signed(&m, &key)).unwrap();
    raw["payload"] = json!(raw["payload"].as_str().unwrap().replace("0.1.0", "9.1.0"));
    assert!(
        update::verify(
            &serde_json::to_vec(&raw).unwrap(),
            &public,
            update::TARGET,
            100
        )
        .is_err()
    );
    m.files[0].path = "../../escape".into();
    assert!(update::verify(&signed(&m, &key), &public, update::TARGET, 100).is_err());
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
