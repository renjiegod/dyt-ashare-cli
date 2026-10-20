use crate::state;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
};
pub const BUNDLED: [(&str, &str); 2] = [
    (
        "dyt-ashare-research",
        include_str!("../skills/dyt-ashare-research/SKILL.md"),
    ),
    (
        "dyt-ashare-quality",
        include_str!("../skills/dyt-ashare-quality/SKILL.md"),
    ),
];
#[derive(Default, Serialize, Deserialize)]
struct Registry {
    entries: Vec<Entry>,
}
#[derive(Serialize, Deserialize)]
struct Entry {
    path: PathBuf,
    sha256: String,
    version: String,
}
pub fn install(root: &Path, dir: &Path) -> Result<Value> {
    fs::create_dir_all(dir)?;
    let dir = fs::canonicalize(dir)?;
    let registry_path = root.join("skills.json");
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("skills.lock"))?;
    fs2::FileExt::lock_exclusive(&lock)?;
    let mut registry = if registry_path.exists() {
        state::read::<Registry>(&registry_path)?
    } else {
        Registry::default()
    };
    let mut installed = Vec::new();
    let mut pending = Vec::new();
    for (name, text) in BUNDLED {
        let folder = dir.join(name);
        let path = folder.join("SKILL.md");
        let hash = state::hash(text.as_bytes());
        // A foreign folder or local edits are not ours to replace, even if the name matches.
        let existing = registry.entries.iter_mut().find(|e| e.path == path);
        let safe = if path.exists() {
            let old = state::hash(&fs::read(&path)?);
            old == hash || existing.as_ref().is_some_and(|e| e.sha256 == old)
        } else {
            !folder.exists() || existing.is_some()
        };
        if !safe {
            pending
                .push(json!({"skill":name,"path":path,"reason":"local_changes_or_foreign_folder"}));
            continue;
        }
        state::atomic(&path, text.as_bytes())?;
        if let Some(e) = existing {
            e.sha256 = hash;
            e.version = env!("CARGO_PKG_VERSION").into();
        } else {
            registry.entries.push(Entry {
                path: path.clone(),
                sha256: hash,
                version: env!("CARGO_PKG_VERSION").into(),
            });
        }
        installed.push(json!({"skill":name,"path":path,"version":env!("CARGO_PKG_VERSION")}));
    }
    state::save(&registry_path, &registry)?;
    let status =
        json!({"installed":installed,"pending":pending,"cli_version":env!("CARGO_PKG_VERSION")});
    state::save(&root.join("skills-status.json"), &status)?;
    Ok(status)
}
pub fn refresh(root: &Path) -> Result<Value> {
    let path = root.join("skills.json");
    if !path.exists() {
        return Ok(json!({"installed":[],"pending":[]}));
    }
    let registry: Registry = state::read(&path)?;
    let dirs = registry
        .entries
        .iter()
        .filter_map(|e| e.path.parent()?.parent().map(Path::to_path_buf))
        .collect::<std::collections::BTreeSet<_>>();
    let mut results = Vec::new();
    for d in dirs {
        results.push(install(root, &d)?);
    }
    Ok(json!({"destinations":results}))
}
pub fn inventory(root: &Path) -> Result<Value> {
    let path = root.join("skills.json");
    if !path.exists() {
        return Ok(json!([]));
    }
    let registry: Registry = state::read(&path)?;
    Ok(json!(registry.entries.iter().map(|e|json!({"path":e.path,"installed_version":e.version,"matches_installed_hash":fs::read(&e.path).ok().is_some_and(|b|state::hash(&b)==e.sha256)})).collect::<Vec<_>>()))
}
pub fn default_dir() -> Result<PathBuf> {
    Ok(state::home()
        .context("Cannot locate Skills directory")?
        .join(".agents/skills"))
}
