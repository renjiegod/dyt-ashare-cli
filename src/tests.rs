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
fn release() -> update::Manifest {
    update::Manifest {
        version: "0.1.0".into(),
        target: update::TARGET.into(),
        expires_at: 200,
        files: [
            update::binary(),
            "skills/dyt-ashare-research/SKILL.md",
            "skills/dyt-ashare-quality/SKILL.md",
        ]
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
    let p = dir.join("dyt-ashare-research/SKILL.md");
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
