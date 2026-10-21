use crate::state;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};

pub const CANONICAL_NAME: &str = "dyt-ashare";
pub const BUNDLED: [(&str, &str); 1] = [(
    CANONICAL_NAME,
    include_str!("../skills/dyt-ashare/SKILL.md"),
)];
const LEGACY_NAMES: [&str; 2] = ["dyt-ashare-research", "dyt-ashare-quality"];

#[derive(Default, Serialize, Deserialize)]
struct Registry {
    entries: Vec<Entry>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Entry {
    path: PathBuf,
    sha256: String,
    version: String,
}

fn read_registry(root: &Path) -> Result<Registry> {
    let path = root.join("skills.json");
    if path.exists() {
        state::read(&path)
    } else {
        Ok(Registry::default())
    }
}

fn lock(root: &Path) -> Result<fs::File> {
    fs::create_dir_all(root)?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join("skills.lock"))?;
    fs2::FileExt::lock_exclusive(&file)?;
    Ok(file)
}

fn entry(registry: &Registry, path: &Path) -> Option<Entry> {
    registry
        .entries
        .iter()
        .find(|entry| entry.path == path)
        .cloned()
}

fn record(registry: &mut Registry, path: &Path, hash: &str) {
    if let Some(existing) = registry.entries.iter_mut().find(|entry| entry.path == path) {
        existing.sha256 = hash.into();
        existing.version = env!("CARGO_PKG_VERSION").into();
    } else {
        registry.entries.push(Entry {
            path: path.to_path_buf(),
            sha256: hash.into(),
            version: env!("CARGO_PKG_VERSION").into(),
        });
    }
}

fn is_legacy(path: &Path) -> bool {
    path.parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .is_some_and(|name| LEGACY_NAMES.contains(&name))
}

fn remove_path(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || metadata.is_file() {
        fs::remove_file(path)?;
    } else {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

fn migrate_legacy(registry: &mut Registry, pending: &mut Vec<Value>, migrated: &mut Vec<Value>) {
    let mut kept = Vec::with_capacity(registry.entries.len());
    for old in registry.entries.drain(..) {
        if !is_legacy(&old.path) {
            kept.push(old);
            continue;
        }
        let current = fs::read(&old.path).ok().map(|bytes| state::hash(&bytes));
        if current.as_deref() == Some(old.sha256.as_str()) {
            if let Err(error) = remove_path(&old.path) {
                pending.push(json!({
                    "skill": old.path.parent().and_then(Path::file_name).and_then(|name| name.to_str()).unwrap_or("legacy"),
                    "path": old.path,
                    "reason": format!("legacy_remove_failed: {error}"),
                }));
                kept.push(old);
            } else {
                if let Some(folder) = old.path.parent() {
                    let _ = fs::remove_dir(folder);
                }
                migrated.push(json!({"path":old.path,"reason":"legacy_skill_removed"}));
            }
        } else if old.path.exists() {
            pending.push(json!({
                "skill": old.path.parent().and_then(Path::file_name).and_then(|name| name.to_str()).unwrap_or("legacy"),
                "path": old.path,
                "reason": "legacy_local_changes",
            }));
            kept.push(old);
        }
    }
    registry.entries = kept;
}

fn safe_to_replace(path: &Path, folder: &Path, previous: Option<&Entry>, hash: &str) -> bool {
    match fs::read(path).ok().map(|bytes| state::hash(&bytes)) {
        Some(current) => current == hash || previous.is_some_and(|entry| entry.sha256 == current),
        None => !folder.exists() || previous.is_some(),
    }
}

fn install_copy(
    registry: &mut Registry,
    dir: &Path,
    installed: &mut Vec<Value>,
    pending: &mut Vec<Value>,
) -> Result<()> {
    for (name, text) in BUNDLED {
        let folder = dir.join(name);
        let path = folder.join("SKILL.md");
        let hash = state::hash(text.as_bytes());
        let previous = entry(registry, &path);
        if symlink_metadata_is_link(&folder) {
            pending.push(json!({"skill":name,"path":path,"reason":"foreign_or_symlinked_folder"}));
            continue;
        }
        if !safe_to_replace(&path, &folder, previous.as_ref(), &hash) {
            pending
                .push(json!({"skill":name,"path":path,"reason":"local_changes_or_foreign_folder"}));
            continue;
        }
        state::atomic(&path, text.as_bytes())?;
        record(registry, &path, &hash);
        installed.push(
            json!({"skill":name,"path":path,"version":env!("CARGO_PKG_VERSION"),"mode":"file"}),
        );
    }
    Ok(())
}

fn symlink_metadata_is_link(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
}

fn relative_path(from: &Path, to: &Path) -> PathBuf {
    let from = from.components().collect::<Vec<_>>();
    let to = to.components().collect::<Vec<_>>();
    let common = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();
    let mut result = PathBuf::new();
    for component in &from[common..] {
        if !matches!(component, Component::CurDir) {
            result.push("..");
        }
    }
    for component in &to[common..] {
        result.push(component.as_os_str());
    }
    result
}

fn create_skill_link(target: &Path, folder: &Path) -> Result<bool> {
    let parent = folder.parent().context("Skill destination has no parent")?;
    fs::create_dir_all(parent)?;
    let link_target = relative_path(parent, target);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&link_target, folder)?;
        Ok(true)
    }
    #[cfg(windows)]
    {
        if std::os::windows::fs::symlink_dir(&link_target, folder).is_ok() {
            return Ok(true);
        }
        fs::create_dir_all(folder)?;
        fs::copy(target.join("SKILL.md"), folder.join("SKILL.md"))?;
        Ok(false)
    }
}

fn install_agent_link(
    registry: &mut Registry,
    dir: &Path,
    canonical_folder: &Path,
    installed: &mut Vec<Value>,
    pending: &mut Vec<Value>,
) -> Result<()> {
    for (name, text) in BUNDLED {
        let folder = dir.join(name);
        let path = folder.join("SKILL.md");
        let parent = folder.parent().context("Skill destination has no parent")?;
        let hash = state::hash(text.as_bytes());
        let previous = entry(registry, &path);
        if symlink_metadata_is_link(&folder) {
            let target = fs::read_link(&folder)?;
            let resolved = if target.is_absolute() {
                target
            } else {
                parent.join(target)
            };
            let same_target = fs::canonicalize(resolved)
                .ok()
                .zip(fs::canonicalize(canonical_folder).ok())
                .is_some_and(|(resolved, canonical)| resolved == canonical);
            if same_target {
                record(registry, &path, &hash);
                installed.push(json!({"skill":name,"path":path,"version":env!("CARGO_PKG_VERSION"),"mode":"symlink"}));
                continue;
            }
            if previous.is_none() || !safe_to_replace(&path, &folder, previous.as_ref(), &hash) {
                pending
                    .push(json!({"skill":name,"path":path,"reason":"foreign_or_wrong_skill_link"}));
                continue;
            }
            remove_path(&folder)?;
        } else if folder.exists() {
            if !safe_to_replace(&path, &folder, previous.as_ref(), &hash) {
                pending.push(
                    json!({"skill":name,"path":path,"reason":"local_changes_or_foreign_folder"}),
                );
                continue;
            }
            state::atomic(&path, text.as_bytes())?;
            record(registry, &path, &hash);
            installed.push(
                json!({"skill":name,"path":path,"version":env!("CARGO_PKG_VERSION"),"mode":"file"}),
            );
            continue;
        }
        match create_skill_link(canonical_folder, &folder)? {
            true => {
                record(registry, &path, &hash);
                installed.push(json!({"skill":name,"path":path,"version":env!("CARGO_PKG_VERSION"),"mode":"symlink"}));
            }
            false => {
                record(registry, &path, &hash);
                installed.push(json!({"skill":name,"path":path,"version":env!("CARGO_PKG_VERSION"),"mode":"file"}));
            }
        }
    }
    Ok(())
}

fn push_destination(destinations: &mut Vec<(PathBuf, String)>, path: PathBuf, label: &str) {
    if !destinations.iter().any(|(existing, _)| existing == &path) {
        destinations.push((path, label.into()));
    }
}

fn configured_root(home: &Path, variable: &str, fallback: &str) -> PathBuf {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(fallback))
}

fn agent_destinations_for_home(home: &Path) -> Vec<(PathBuf, String)> {
    let mut destinations = Vec::new();
    push_destination(&mut destinations, home.join(".agents/skills"), "universal");
    let candidates = [
        (
            configured_root(home, "CLAUDE_CONFIG_DIR", ".claude"),
            "claude-code",
        ),
        (configured_root(home, "CODEX_HOME", ".codex"), "codex"),
        (home.join(".cursor"), "cursor"),
        (home.join(".windsurf"), "windsurf"),
        (home.join(".codeium/windsurf"), "windsurf"),
        (home.join(".gemini"), "gemini"),
        (home.join(".gemini/antigravity"), "antigravity"),
        (home.join(".trae"), "trae"),
        (home.join(".trae-cn"), "trae-cn"),
        (home.join(".qwen"), "qwen"),
        (home.join(".openclaw"), "openclaw"),
        (home.join(".clawdbot"), "openclaw"),
        (home.join(".moltbot"), "openclaw"),
        (home.join(".roo"), "roo-code"),
        (
            configured_root(home, "WORKBUDDY_CONFIG_DIR", ".workbuddy-ai"),
            "workbuddy",
        ),
        (home.join(".workbuddy"), "workbuddy"),
    ];
    for (root, label) in candidates {
        let dir = root.join("skills");
        if root.exists() || dir.exists() {
            push_destination(&mut destinations, dir, label);
        }
    }
    let config_home = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    let open_code = config_home.join("opencode");
    if open_code.exists() || open_code.join("skills").exists() {
        push_destination(&mut destinations, open_code.join("skills"), "opencode");
    }
    destinations
}

fn finish(
    root: &Path,
    registry: &Registry,
    installed: Vec<Value>,
    pending: Vec<Value>,
    migrated: Vec<Value>,
) -> Result<Value> {
    state::save(&root.join("skills.json"), registry)?;
    let status = json!({
        "skill": CANONICAL_NAME,
        "installed": installed,
        "pending": pending,
        "migrated": migrated,
        "cli_version": env!("CARGO_PKG_VERSION"),
    });
    state::save(&root.join("skills-status.json"), &status)?;
    Ok(status)
}

pub fn install(root: &Path, dir: &Path) -> Result<Value> {
    let _lock = lock(root)?;
    let dir = fs::canonicalize(dir).or_else(|_| {
        fs::create_dir_all(dir)?;
        fs::canonicalize(dir)
    })?;
    let mut registry = read_registry(root)?;
    let mut installed = Vec::new();
    let mut pending = Vec::new();
    let mut migrated = Vec::new();
    migrate_legacy(&mut registry, &mut pending, &mut migrated);
    install_copy(&mut registry, &dir, &mut installed, &mut pending)?;
    finish(root, &registry, installed, pending, migrated)
}

fn install_targets(root: &Path, home: &Path, include_registered: bool) -> Result<Value> {
    let _lock = lock(root)?;
    let mut destinations = agent_destinations_for_home(home);
    if include_registered {
        for entry in read_registry(root)?.entries {
            if let Some(dir) = entry.path.parent().and_then(Path::parent)
                && !destinations.iter().any(|(candidate, _)| candidate == dir)
            {
                destinations.push((dir.to_path_buf(), "custom".into()));
            }
        }
    }
    let canonical_dir = home.join(".agents/skills");
    let canonical_folder = canonical_dir.join(CANONICAL_NAME);
    let mut registry = read_registry(root)?;
    let mut installed = Vec::new();
    let mut pending = Vec::new();
    let mut migrated = Vec::new();
    migrate_legacy(&mut registry, &mut pending, &mut migrated);
    for (dir, label) in destinations {
        if dir == canonical_dir || label == "custom" {
            install_copy(&mut registry, &dir, &mut installed, &mut pending)?;
        } else {
            install_agent_link(
                &mut registry,
                &dir,
                &canonical_folder,
                &mut installed,
                &mut pending,
            )?;
        }
    }
    finish(root, &registry, installed, pending, migrated)
}

pub fn install_default(root: &Path) -> Result<Value> {
    let home = state::home()?;
    install_targets(root, &home, false)
}

pub fn refresh(root: &Path) -> Result<Value> {
    let home = state::home()?;
    let status = install_targets(root, &home, true)?;
    Ok(json!({"destinations":[status]}))
}

pub fn inventory(root: &Path) -> Result<Value> {
    let path = root.join("skills.json");
    if !path.exists() {
        return Ok(json!([]));
    }
    let registry: Registry = state::read(&path)?;
    Ok(json!(registry
        .entries
        .iter()
        .map(|entry| {
            let parent = entry.path.parent().unwrap_or(Path::new(""));
            json!({
                "skill":CANONICAL_NAME,
                "path":entry.path,
                "installed_version":entry.version,
                "matches_installed_hash":fs::read(&entry.path).ok().is_some_and(|bytes|state::hash(&bytes)==entry.sha256),
                "is_symlink":symlink_metadata_is_link(parent),
            })
        })
        .collect::<Vec<_>>()))
}

#[cfg(test)]
pub(crate) fn destinations_for_test(home: &Path) -> Vec<PathBuf> {
    agent_destinations_for_home(home)
        .into_iter()
        .map(|(path, _)| path)
        .collect()
}

#[cfg(test)]
pub(crate) fn install_for_test(root: &Path, home: &Path) -> Result<Value> {
    install_targets(root, home, false)
}
