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
            Err(_) => {
                // First run: persist an empty registry so the file location
                // is discoverable by the user.
                let reg = Self::default();
                reg.save();
                reg
            }
        }
    }

    pub fn save(&self) {
        let path = registry_path();
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&path, json) {
                    log::warn!("[workspace] failed to write {}: {}", path.display(), e);
                }
            }
            Err(e) => log::warn!("[workspace] failed to serialize registry: {}", e),
        }
    }

    pub fn project_by_id(&self, id: &str) -> Option<&ProjectEntry> {
        self.projects
            .iter()
            .find(|p| p.enabled && p.id == id)
    }

    /// Resolve a scope argument ("liri" | "active_project") to a root path.
    /// `active_project` resolves against the environment observer's current
    /// project hint matched by name; unknown/None hint → None.
    pub fn resolve_scope(&self, scope: &str) -> Option<PathBuf> {
        if scope == "active_project" {
            let hint = crate::perception::environment::current_hints().project_hint;
            let hint = hint?;
            let entry = self
                .projects
                .iter()
                .find(|p| p.enabled && (p.name == hint || p.id == hint))?;
            Some(PathBuf::from(&entry.path))
        } else {
            self.project_by_id(scope).map(|p| PathBuf::from(&p.path))
        }
    }

    pub fn enabled_projects(&self) -> Vec<&ProjectEntry> {
        self.projects.iter().filter(|p| p.enabled).collect()
    }
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
    fn liri_dir_under_app_data() {
        let dir = liri_dir();
        assert!(dir.ends_with(".liri"));
    }
}
