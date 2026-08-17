//! Workspace registry (plan 2026-08-17 §2.6): the `.liri/` anchor under the
//! app data dir. A PURE registry — project id/path/name/description/enabled.
//! No file lists, no freshness problem: real directory contents are queried
//! by the Observe tools at call time. Semantic layer (current task, doc
//! entry points) lives in human-editable `.liri/PROJECTS/*.md`.
//!
//! Authorization is deliberately NOT stored here — grants live in the
//! `fs_grants` table (the "two stores" ruling, §2.7). The registry says
//! "which worlds exist"; grants say "which she may read".

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The `.liri/` directory (under the app data dir, never in any project).
pub fn liri_dir() -> PathBuf {
    crate::config::app_data_dir().join(".liri")
}

fn registry_path() -> PathBuf {
    liri_dir().join("workspace-index.json")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectEntry {
    pub id: String,
    /// Real path on disk — projects are NEVER copied into `.liri/`.
    pub path: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceRegistry {
    #[serde(default)]
    pub projects: Vec<ProjectEntry>,
}

/// Stable, collision-free id for an auto-registered root (U3): lowercase
/// ASCII/digits, other chars become '-'; "-2, -3…" on collision.
fn unique_project_id(reg: &WorkspaceRegistry, name: &str) -> String {
    let mut base: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    while base.starts_with('-') {
        base.remove(0);
    }
    while base.ends_with('-') {
        base.pop();
    }
    if base.is_empty() {
        base.push_str("project");
    }
    let mut id = base.clone();
    let mut n = 2usize;
    while reg.projects.iter().any(|p| p.id == id) {
        id = format!("{}-{}", base, n);
        n += 1;
    }
    id
}

impl WorkspaceRegistry {
    /// Load from disk, creating `.liri/` + `PROJECTS/` + an empty registry on
    /// first run. Corrupt JSON degrades to an empty registry (logged) rather
    /// than failing the caller — the registry is an index, not critical state.
    pub fn load() -> Self {
        let dir = liri_dir();
        if let Err(e) = std::fs::create_dir_all(dir.join("PROJECTS")) {
            log::warn!("[workspace] failed to create {}: {}", dir.display(), e);
        }
        match std::fs::read_to_string(registry_path()) {
            Ok(text) => match serde_json::from_str(&text) {
                Ok(reg) => reg,
                Err(e) => {
                    log::warn!("[workspace] registry corrupt ({}), starting empty", e);
                    Self::default()
                }
            },
            // First run only: persist an empty registry so the file location
            // is discoverable. ANY other read failure (permission / sharing
            // violation) degrades to an empty in-memory registry WITHOUT
            // saving — a transient IO error must never overwrite the user's
            // file with an empty table (plan §8.2-H2).
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let reg = Self::default();
                reg.save();
                reg
            }
            Err(e) => {
                log::warn!(
                    "[workspace] registry unreadable ({}), starting empty WITHOUT overwriting",
                    e
                );
                Self::default()
            }
        }
    }

    pub fn save(&self) {
        let path = registry_path();
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                // H2 hardening: temp + rename — a crash mid-write must never
                // leave a truncated JSON that degrades every project on boot.
                let tmp = path.with_extension("json.tmp");
                if let Err(e) = std::fs::write(&tmp, &json) {
                    log::warn!("[workspace] failed to write {}: {}", tmp.display(), e);
                    return;
                }
                if let Err(e) = std::fs::rename(&tmp, &path) {
                    log::warn!(
                        "[workspace] failed to rename {} -> {}: {}",
                        tmp.display(),
                        path.display(),
                        e
                    );
                    let _ = std::fs::remove_file(&tmp);
                }
            }
            Err(e) => log::warn!("[workspace] failed to serialize registry: {}", e),
        }
    }

    /// U3 (plan §8.4): the first granted root for an unregistered location is
    /// promoted to an enabled project so it becomes addressable ("把它的
    /// git 状态给我" / active_project resolution). No-op when the root already
    /// belongs to a registered project. Registry stays an INDEX — never reads
    /// or stores file contents here.
    pub fn register_granted_root(&mut self, root: &str) -> Option<String> {
        let canonical = std::fs::canonicalize(root).ok()?;
        let path_str = canonical.to_string_lossy().to_string();
        // Already inside a registered project — no new world needed.
        if owning_project(self, &canonical).is_some() {
            return None;
        }
        // Exact path already exists (even disabled) — leave it untouched.
        if let Some(p) = self.projects.iter().find(|p| p.path == path_str) {
            return Some(p.id.clone());
        }
        let name = canonical
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "project".to_string());
        let id = unique_project_id(self, &name);
        let entry = ProjectEntry {
            id,
            path: path_str,
            name,
            description: String::new(),
            enabled: true,
        };
        self.projects.push(entry);
        let _ = std::fs::create_dir_all(liri_dir().join("PROJECTS"));
        self.save();
        self.projects.last().map(|p| p.id.clone())
    }

    pub fn project_by_id(&self, id: &str) -> Option<&ProjectEntry> {
        self.projects
            .iter()
            .find(|p| p.enabled && p.id == id)
    }

    /// Resolve a scope argument ("liri" | "active_project") to a root path.
    /// `active_project` resolves against the environment observer's current
    /// project hint matched against name OR id, case-insensitively (§8.5-M5);
    /// unknown/None hint → None.
    pub fn resolve_scope(&self, scope: &str) -> Option<PathBuf> {
        if scope == "active_project" {
            let hint = crate::perception::environment::current_hints().project_hint;
            let hint = hint?;
            let entry = project_by_name_ci(self, &hint)?;
            Some(PathBuf::from(&entry.path))
        } else {
            self.project_by_id(scope).map(|p| PathBuf::from(&p.path))
        }
    }

    pub fn enabled_projects(&self) -> Vec<&ProjectEntry> {
        self.projects.iter().filter(|p| p.enabled).collect()
    }
}

/// U3 entry point used by the consent resolver: load, ensure the granted root
/// has a workspace world, persist atomically. Returns the project id created
/// (or already owning), None when the root can't canonicalize.
pub fn register_root_after_grant(root: &str) -> Option<String> {
    let mut reg = WorkspaceRegistry::load();
    let id = reg.register_granted_root(root);
    if let Some(id) = &id {
        log::info!("[workspace] U3: granted root {} -> project {}", root, id);
    }
    id
}

/// Match an observer project hint against an enabled project's name OR id,
/// case-insensitively — "Liri"、"liri"、"LIRI" 指向同一个项目。
fn project_by_name_ci<'a>(
    registry: &'a WorkspaceRegistry,
    hint: &str,
) -> Option<&'a ProjectEntry> {
    let h = normalize_for_compare(hint);
    registry.projects.iter().find(|p| {
        p.enabled
            && (normalize_for_compare(&p.name) == h
                || normalize_for_compare(&p.id) == h)
    })
}

/// The project root a canonical path belongs to, if any registered project
/// contains it (prefix match, case-insensitive — canonicalize normalizes
/// case on Windows, this is defense when it hasn't run yet).
pub fn owning_project<'a>(
    registry: &'a WorkspaceRegistry,
    path: &Path,
) -> Option<&'a ProjectEntry> {
    let p_str = normalize_for_compare(&path.to_string_lossy());
    registry
        .enabled_projects()
        .into_iter()
        .find(|proj| {
            let root = normalize_for_compare(&proj.path);
            !root.is_empty() && (p_str == root || p_str.starts_with(&format!("{}\\", root)) || p_str.starts_with(&format!("{}/", root)))
        })
}

/// Lowercase + forward slashes, no trailing separator — for prefix compares.
pub fn normalize_for_compare(path: &str) -> String {
    let mut s = path.to_lowercase().replace('/', "\\");
    while s.ends_with('\\') {
        s.pop();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg() -> WorkspaceRegistry {
        WorkspaceRegistry {
            projects: vec![ProjectEntry {
                id: "liri".into(),
                path: "D:\\Projects\\Liri".into(),
                name: "Liri".into(),
                description: String::new(),
                enabled: true,
            }],
        }
    }

    #[test]
    fn owning_project_prefix_match() {
        let r = reg();
        assert!(owning_project(&r, Path::new("D:\\Projects\\Liri\\src\\main.rs")).is_some());
        assert!(owning_project(&r, Path::new("D:\\Projects\\Liri")).is_some());
        // Sibling directory with a shared prefix must NOT match.
        assert!(owning_project(&r, Path::new("D:\\Projects\\LiriClone\\x.rs")).is_none());
        assert!(owning_project(&r, Path::new("E:\\elsewhere")).is_none());
    }

    #[test]
    fn owning_project_case_insensitive() {
        let r = reg();
        assert!(owning_project(&r, Path::new("d:\\projects\\liri\\src")).is_some());
    }

    #[test]
    fn disabled_project_ignored() {
        let mut r = reg();
        r.projects[0].enabled = false;
        assert!(owning_project(&r, Path::new("D:\\Projects\\Liri\\a.rs")).is_none());
        assert!(r.resolve_scope("liri").is_none());
    }

    #[test]
    fn resolve_scope_by_id() {
        let r = reg();
        assert_eq!(
            r.resolve_scope("liri").unwrap(),
            PathBuf::from("D:\\Projects\\Liri")
        );
        assert!(r.resolve_scope("unknown").is_none());
        // active_project with no observer hint → None (tolerable, no panic).
        assert!(r.resolve_scope("active_project").is_none());
    }

    #[test]
    fn project_hint_match_is_case_insensitive() {
        let r = reg();
        for hint in ["liri", "LIRI", "LiRi"] {
            let hit = project_by_name_ci(&r, hint).expect("case variant must match");
            assert_eq!(hit.id, "liri");
        }
        assert!(project_by_name_ci(&r, "别的项目").is_none());
        let mut disabled = r;
        disabled.projects[0].enabled = false;
        assert!(project_by_name_ci(&disabled, "liri").is_none());
    }

    #[test]
    fn liri_dir_under_app_data() {
        let dir = liri_dir();
        assert!(dir.ends_with(".liri"));
    }
}
