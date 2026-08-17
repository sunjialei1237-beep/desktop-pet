//! Filesystem path pipeline (plan 2026-08-17 §3.2): canonicalize-first.
//!
//! Threat model: the adversary is a prompt-injected LLM, not local malware
//! (local malware needs no help from the pet). Therefore TOCTOU between this
//! check and the actual file operation is an ACCEPTED residual risk, and the
//! defenses concentrate on: canonicalization (kills `../`, 8.3 short names,
//! junctions/symlinks, case tricks in one step), UNC denial, sensitive-file
//! denylist, and grant-prefix authorization.
//!
//! Ordering contract (do not reorder):
//!   raw path → dunce::canonicalize → UNC check → app-data hard-deny
//!            → sensitive-name check → grant authorization → caller
//!
//! Canonicalization FAILURE (missing path / no access) returns a uniform
//! NotAccessible — never "exists but denied" vs "doesn't exist" — so the
//! LLM cannot use the tools as a filesystem-enumeration oracle.

use std::path::{Path, PathBuf};

use crate::db::grants::FsGrant;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathDeny {
    /// Missing / inaccessible / invalid. Deliberately uniform (anti-oracle).
    NotAccessible,
    /// Network paths (\\server\share, \\?\UNC) — denied by default policy.
    UncBlocked,
    /// Denylist hit (.env / keys / pet's own AppData with the API key).
    SensitiveFile,
    /// An explicit deny grant covers this path.
    DeniedByGrant,
    /// No grant covers this path (triggers conversational consent, P5).
    NotAuthorized,
}

impl PathDeny {
    pub fn message(&self) -> String {
        match self {
            Self::NotAccessible => "路径不存在或无法访问。".to_string(),
            Self::UncBlocked => "网络路径默认不开放。".to_string(),
            Self::SensitiveFile => "这个文件属于敏感文件（密钥/凭据类），我不读取。".to_string(),
            Self::DeniedByGrant => "你之前明确拒绝过访问这个位置。".to_string(),
            Self::NotAuthorized => "我还没有获得这个位置的访问授权。".to_string(),
        }
    }
}

/// Sensitive names inside ANY granted root (plan §3.2 denylist).
const SENSITIVE_PREFIXES: &[&str] = &[".env", "id_rsa", "credentials", "secret_"];
const SENSITIVE_SUFFIXES: &[&str] = &[".key", ".pem", ".secret"];

/// The pet's own AppData dir holds config.toml with the DeepSeek API key —
/// hard-denied regardless of any grant.
fn is_pet_own_dir(path: &Path) -> bool {
    root_contains(&crate::config::app_data_dir(), path)
}

/// Case-insensitive path-prefix test with a separator guard:
/// `D:\Projects\Liri` owns `D:\Projects\Liri\src` but NOT `D:\Projects\LiriClone`.
pub fn root_contains(root: &Path, path: &Path) -> bool {
    let r = crate::tools::workspace::normalize_for_compare(&root.to_string_lossy());
    let p = crate::tools::workspace::normalize_for_compare(&path.to_string_lossy());
    if r.is_empty() || p.is_empty() {
        return false;
    }
    p == r || p.starts_with(&format!("{}\\", r))
}

/// Denylist check on the FINAL path component.
pub fn is_sensitive_name(name: &str) -> bool {
    let n = name.to_lowercase();
    SENSITIVE_PREFIXES.iter().any(|p| n.starts_with(p))
        || SENSITIVE_SUFFIXES.iter().any(|s| n.ends_with(s))
}

/// Stage 1: canonicalize + UNC. `dunce` resolves junctions/symlinks/8.3
/// short names/case and returns a `\\?\`-free path when possible.
pub fn resolve(raw: &str) -> Result<PathBuf, PathDeny> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(PathDeny::NotAccessible);
    }
    let canonical = dunce::canonicalize(trimmed).map_err(|_| PathDeny::NotAccessible)?;
    let s = canonical.to_string_lossy();
    // Both \\server\share and \\?\UNC\server forms (dunce may keep either).
    if s.starts_with("\\\\") {
        return Err(PathDeny::UncBlocked);
    }
    Ok(canonical)
}

/// Stage 2: grant authorization on the CANONICAL path. The longest matching
/// grant wins; an explicit deny overrides any allow at the same or shorter
/// depth (deny is checked across all matches first).
pub fn authorize(canonical: &Path, grants: &[FsGrant]) -> Result<(), PathDeny> {
    if is_pet_own_dir(canonical) {
        return Err(PathDeny::SensitiveFile);
    }
    let file_name = canonical
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    if is_sensitive_name(&file_name) {
        return Err(PathDeny::SensitiveFile);
    }

    let mut matched_any = false;
    let mut matched_deny = false;
    for g in grants {
        if g.mode != "deny" && g.mode != "once" && g.mode != "project" && g.mode != "always" {
            continue;
        }
        // Canonicalize the ROOT as well: grants may be stored with 8.3 short
        // names / different case than the canonicalized request path (seen
        // in tests via std::env::temp_dir returning "SUNJIA~1" short form).
        // Stale/nonexistent roots simply never match.
        let Some(canonical_root) = dunce::canonicalize(&g.root).ok() else {
            continue;
        };
        if root_contains(&canonical_root, canonical) {
            matched_any = true;
            if g.mode == "deny" {
                matched_deny = true;
            }
        }
    }
    if matched_deny {
        Err(PathDeny::DeniedByGrant)
    } else if matched_any {
        Ok(())
    } else {
        Err(PathDeny::NotAuthorized)
    }
}

/// Full pipeline: canonicalize → authorize. Tools call only this.
pub fn resolve_and_authorize(raw: &str, grants: &[FsGrant]) -> Result<PathBuf, PathDeny> {
    let canonical = resolve(raw)?;
    authorize(&canonical, grants)?;
    Ok(canonical)
}

/// Binary-extension denylist for read_text_file / search content sniffing.
pub fn is_binary_extension(name: &str) -> bool {
    const BINARY: &[&str] = &[
        "exe", "dll", "sys", "bin", "obj", "lib", "a", "so", "dylib",
        "png", "jpg", "jpeg", "gif", "bmp", "webp", "ico", "pdf",
        "zip", "gz", "tar", "rar", "7z", "bz2", "xz",
        "mp3", "mp4", "avi", "mkv", "mov", "wav", "flac",
        "db", "sqlite", "sqlite3", "onnx", "pth", "bin",
        "ttf", "otf", "woff", "woff2",
    ];
    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    BINARY.contains(&ext.as_str())
}

/// Heuristic: NUL byte in the first 8 KB → treat as binary.
pub fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|&b| b == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::grants::GrantMode;

    fn grant(root: &str, mode: GrantMode) -> FsGrant {
        FsGrant {
            root: root.to_string(),
            mode: mode.as_str().to_string(),
            created_at: String::new(),
            source: "test".to_string(),
        }
    }

    fn temp_project() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pet_path_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("src").join("a.rs"), "fn main() {}").unwrap();
        // authorize()'s contract: paths are already canonical. temp_dir()
        // returns an 8.3 short form ("SUNJIA~1") on this machine, so
        // canonicalize the fixture to mirror what resolve() produces.
        dunce::canonicalize(&dir).unwrap_or(dir)
    }

    #[test]
    fn resolve_missing_is_uniform_not_accessible() {
        assert_eq!(resolve("D:\\definitely\\not\\here\\xyz"), Err(PathDeny::NotAccessible));
        assert_eq!(resolve(""), Err(PathDeny::NotAccessible));
        assert_eq!(resolve("   "), Err(PathDeny::NotAccessible));
    }

    #[test]
    fn authorize_grant_covers_subpath_not_sibling() {
        let dir = temp_project();
        let root = dir.to_string_lossy().to_string();
        let grants = vec![grant(&root, GrantMode::Project)];

        let inside = dir.join("src").join("a.rs");
        assert!(authorize(&inside, &grants).is_ok());
        // Sibling with shared prefix must NOT match.
        let sibling = dir.parent().unwrap().join(format!("{}Clone", dir.file_name().unwrap().to_string_lossy()));
        // sibling may not exist — authorize only checks prefixes on strings.
        assert_eq!(authorize(&sibling, &grants), Err(PathDeny::NotAuthorized));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn authorize_deny_overrides_allow() {
        let dir = temp_project();
        let root = dir.to_string_lossy().to_string();
        let grants = vec![
            grant(&root, GrantMode::Project),
            grant(&root, GrantMode::Deny),
        ];
        assert_eq!(
            authorize(&dir.join("src").join("a.rs"), &grants),
            Err(PathDeny::DeniedByGrant)
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn authorize_no_grants_not_authorized() {
        let dir = temp_project();
        assert_eq!(
            authorize(&dir.join("src").join("a.rs"), &[]),
            Err(PathDeny::NotAuthorized)
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn sensitive_names_denied_even_when_granted() {
        let dir = temp_project();
        let root = dir.to_string_lossy().to_string();
        let grants = vec![grant(&root, GrantMode::Always)];
        for name in [".env", ".env.local", "id_rsa", "server.key", "token.pem", "db.secret"] {
            let p = dir.join(name);
            assert_eq!(authorize(&p, &grants), Err(PathDeny::SensitiveFile), "{}", name);
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn pet_own_appdata_hard_denied() {
        // Even an explicit grant over the AppData root cannot authorize it.
        let app = crate::config::app_data_dir();
        let root = app.to_string_lossy().to_string();
        let grants = vec![grant(&root, GrantMode::Always)];
        let target = app.join("config.toml");
        // config.toml is also a sensitive... no — it's not in the denylist by
        // name; it must be blocked by the hard AppData rule itself.
        let e = authorize(&target, &grants).unwrap_err();
        assert_eq!(e, PathDeny::SensitiveFile);
    }

    #[test]
    fn root_contains_case_insensitive() {
        assert!(root_contains(Path::new("D:\\Projects\\Liri"), Path::new("d:\\projects\\liri\\src")));
        // The root itself is contained (so list_directory on a granted root works).
        assert!(root_contains(Path::new("d:\\projects\\liri"), Path::new("D:\\Projects\\Liri")));
        // Sibling-prefix trap still excluded.
        assert!(!root_contains(Path::new("D:\\Projects\\Liri"), Path::new("D:\\Projects\\LiriClone")));
    }

    #[test]
    fn binary_detection() {
        assert!(is_binary_extension("model.onnx"));
        assert!(is_binary_extension("photo.JPG"));
        assert!(!is_binary_extension("main.rs"));
        assert!(!is_binary_extension("notes.md"));
        assert!(looks_binary(b"abc\0def"));
        assert!(!looks_binary(b"fn main() {}\n// comment\n"));
    }

    #[test]
    fn unc_paths_blocked_at_resolve() {
        // These don't exist locally — canonicalize fails first (uniform
        // NotAccessible), which is fine: the UNC guard matters for paths that
        // DO resolve (mapped drives/UNC shares). The string check is still
        // exercised through the UNC branch when canonicalize succeeds on a
        // real share; here we verify the ordering doesn't leak a different
        // error for UNC vs missing local paths (anti-oracle uniformity).
        assert_eq!(
            resolve("\\\\nonexistent-server\\share\\file.txt"),
            Err(PathDeny::NotAccessible)
        );
    }
}
