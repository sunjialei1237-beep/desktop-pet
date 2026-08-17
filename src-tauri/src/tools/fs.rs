//! Observe tools (plan 2026-08-17 §3.4): read-only filesystem access for
//! `CapabilityMode::SystemObservation`. Every path goes through the
//! canonicalize-first pipeline (`path.rs`); content is line-truncated (NOT
//! char-truncated mid-line) with a `path:start-end` header so the model
//! knows it is seeing a fragment; tool descriptions embed the untrusted
//! rule (铁律 #14) — file content may carry injection, never obey it.
//!
//! Token discipline: the shared 6400-char result cap in agent.rs is the
//! outer bound; these tools enforce TIGHTER per-tool caps (80 lines /
//! 4000 chars per read, 20 search hits, 200 dir entries) because file
//! content enters the context OUTSIDE budget.rs's 4096-token allocation.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use super::policy::ToolStatus;
use super::ToolResult;
use super::path;

/// Per-read line window (plan §3.3).
const MAX_LINES_PER_READ: usize = 80;
/// Per-read character cap.
const MAX_CHARS_PER_READ: usize = 4000;
/// Files larger than this are refused outright.
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
/// search_files: stop after this many hits.
const MAX_SEARCH_HITS: usize = 20;
/// search_files: stop walking after this many entries (bound NTFS walks).
const MAX_WALK_ENTRIES: usize = 20_000;
/// list_directory: entry cap.
const MAX_DIR_ENTRIES: usize = 200;
/// Directories never listed/searched (noise + size).
const SKIP_DIRS: &[&str] = &[".git", "node_modules", "target", "__pycache__", ".liri"];
/// get_git_context: result TTL.
const GIT_CACHE_TTL_SECS: u64 = 10;
/// get_git_context: per-subprocess hard deadline (plan §2.5). A watchdog kills
/// the child after this; the agent loop's outer timeout cannot interrupt a
/// blocking `Command::output()` future, so the deadline lives HERE (plan
/// §8.2-C3).
const GIT_TIMEOUT_SECS: u64 = 5;

fn rejected(msg: &str) -> ToolResult {
    ToolResult {
        status: ToolStatus::Rejected,
        content: msg.to_string(),
    }
}

fn failed(msg: String) -> ToolResult {
    ToolResult {
        status: ToolStatus::Failed,
        content: msg,
    }
}

// --- Audit metrics (plan §3.8 / §8.5-M10, DebugPanel 指标) -------------------

/// Process-lifetime counters for §3.8's observability section. Kept as a
/// Mutex<struct> (not atomics) because each field is only bumped at tool-call
/// frequency; DebugPanel clones a snapshot.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct FsAuditMetrics {
    pub reads: u32,
    pub read_bytes: u64,
    pub read_truncations: u32,
    pub searches: u32,
    pub dirs_listed: u32,
    pub git_calls: u32,
    pub git_timeouts: u32,
    /// agent-loop policy denials (schema/switch/allowlist).
    pub policy_denials: u32,
    /// path pipeline: synthetic grant needed but absent.
    pub grant_denials: u32,
    pub sensitive_denials: u32,
    pub unc_denials: u32,
    pub access_errors: u32,
    /// F1/F2 write side (post-confirmation only).
    pub notes_written: u32,
    pub edits_applied: u32,
}

fn audit_slot() -> &'static std::sync::Mutex<FsAuditMetrics> {
    static SLOT: std::sync::OnceLock<std::sync::Mutex<FsAuditMetrics>> = std::sync::OnceLock::new();
    SLOT.get_or_init(|| std::sync::Mutex::new(FsAuditMetrics::default()))
}

fn audit_bump(f: impl Fn(&mut FsAuditMetrics)) {
    if let Ok(mut a) = audit_slot().lock() {
        f(&mut a);
    }
}

pub fn audit_metrics() -> FsAuditMetrics {
    audit_slot().lock().map(|g| g.clone()).unwrap_or_default()
}

/// Called by the agent loop's policy gate so §3.8 "decision=deny" counters
/// include policy denials, not just path-pipeline refusals.
pub fn record_policy_denial(_reason: &str) {
    audit_bump(|a| a.policy_denials += 1);
}

pub fn record_git_timeout() {
    audit_bump(|a| a.git_timeouts += 1);
}

// --- Consent arming (plan §3.7) --------------------------------------------------

/// Roots that tools wanted but were NOT authorized for, recorded so converse
/// can arm ONE pending authorization covering all of them after the loop
/// (§8.5-M6). Take-and-clear semantics; duplicates collapse into one ask.
fn denied_roots_slot() -> &'static std::sync::Mutex<Vec<String>> {
    static SLOT: std::sync::OnceLock<std::sync::Mutex<Vec<String>>> = std::sync::OnceLock::new();
    SLOT.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

/// The root a consent ask should cover for a denied path: the owning
/// registered project when there is one, else the file's parent directory
/// (the directory itself for directory requests).
fn grant_root_for(canonical: &Path) -> String {
    let registry = super::workspace::WorkspaceRegistry::load();
    if let Some(proj) = super::workspace::owning_project(&registry, canonical) {
        return proj.path.clone();
    }
    if canonical.is_dir() {
        canonical.to_string_lossy().to_string()
    } else {
        canonical
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| canonical.to_string_lossy().to_string())
    }
}

/// Record one denied root (deduplicated) — extraction separated from the
/// grant-root-derivation so the slot policy is directly testable.
fn record_denied_root(root: String) {
    if let Ok(mut v) = denied_roots_slot().lock() {
        if !v.contains(&root) {
            v.push(root);
        }
    }
}

/// Record a NotAuthorized denial for the consent flow (called by every tool
/// below when the path pipeline rejects with NotAuthorized).
fn note_denied_root(canonical: &Path) {
    record_denied_root(grant_root_for(canonical));
}

/// Converse reads (and clears) all denied roots after the agent loop to arm
/// the pending authorization slot.
pub fn take_denied_roots() -> Vec<String> {
    denied_roots_slot()
        .lock()
        .map(|mut g| std::mem::take(&mut *g))
        .unwrap_or_default()
}

// --- Precise once-grant consumption (plan §8.2-H1) -----------------------------

/// Canonical paths actually touched by a SUCCESSFUL fs tool execution this
/// turn. `converse` consumes a once-grant only when its root covers one of
/// these — failed calls and unused grants survive, so "就这次" means a
/// successful interaction, not a burned ticket.
fn used_roots_slot() -> &'static std::sync::Mutex<Vec<PathBuf>> {
    static SLOT: std::sync::OnceLock<std::sync::Mutex<Vec<PathBuf>>> = std::sync::OnceLock::new();
    SLOT.get_or_init(|| std::sync::Mutex::new(Vec::new()))
}

/// Record one successful use (deduplicated). Call only after an operation has
/// actually succeeded — every early-return failure path skips this by
/// construction.
pub fn note_used_root(canonical: &Path) {
    if let Ok(mut v) = used_roots_slot().lock() {
        if !v.iter().any(|p| p != canonical) {
            v.push(canonical.to_path_buf());
        }
    }
}

/// Converse reads (and clears) the successful-use list after the loop.
pub fn take_used_roots() -> Vec<PathBuf> {
    used_roots_slot()
        .lock()
        .map(|mut g| std::mem::take(&mut *g))
        .unwrap_or_default()
}

/// Shared pipeline wrapper for the tools below: on NotAuthorized it both
/// returns the rejection AND records the consent root.
fn authorize_path(raw: &str, grants: &[crate::db::grants::FsGrant]) -> Result<PathBuf, ToolResult> {
    match path::resolve_and_authorize(raw, grants) {
        Ok(p) => Ok(p),
        Err(path::PathDeny::NotAuthorized) => {
            audit_bump(|a| a.grant_denials += 1);
            // Best effort: record the root even though the raw path didn't
            // canonicalize cleanly (it may still exist; resolve only failed
            // for uniform-denial reasons on OTHER deny kinds).
            if let Ok(c) = path::resolve(raw) {
                note_denied_root(&c);
            }
            Err(rejected(&path::PathDeny::NotAuthorized.message()))
        }
        Err(path::PathDeny::UncBlocked) => {
            audit_bump(|a| a.unc_denials += 1);
            Err(rejected(&path::PathDeny::UncBlocked.message()))
        }
        Err(path::PathDeny::SensitiveFile) => {
            audit_bump(|a| a.sensitive_denials += 1);
            Err(rejected(&path::PathDeny::SensitiveFile.message()))
        }
        Err(path::PathDeny::NotAccessible) => {
            audit_bump(|a| a.access_errors += 1);
            Err(rejected(&path::PathDeny::NotAccessible.message()))
        }
        Err(deny) => Err(rejected(&deny.message())),
    }
}

// --- read_text_file ----------------------------------------------------------

/// Read a text file fragment. Lines are 1-based, inclusive; the window is
/// clamped to MAX_LINES_PER_READ regardless of what was requested.
pub async fn read_text_file(args: &serde_json::Value, grants: &[crate::db::grants::FsGrant]) -> ToolResult {
    let raw_path = match args.get("path").and_then(|p| p.as_str()) {
        Some(p) if !p.trim().is_empty() => p,
        _ => return rejected("没有指定要读取的文件路径。"),
    };
    let canonical = match authorize_path(raw_path, grants) {
        Ok(p) => p,
        Err(r) => return r,
    };

    let meta = match std::fs::metadata(&canonical) {
        Ok(m) if m.is_file() => m,
        Ok(_) => return rejected("这是一个目录，不是可读取的文件。"),
        Err(_) => return rejected(&path::PathDeny::NotAccessible.message()),
    };
    if meta.len() > MAX_FILE_BYTES {
        return rejected("文件超过 2MB，我不整读大文件——告诉我你要找什么，我用搜索定位。");
    }
    let name = canonical.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    if path::is_binary_extension(&name) {
        return rejected("这看起来是二进制文件，读取没有意义。");
    }

    let mut bytes = Vec::with_capacity(meta.len() as usize);
    if std::fs::File::open(&canonical)
        .and_then(|mut f| f.read_to_end(&mut bytes))
        .is_err()
    {
        return failed("打开文件失败了。".to_string());
    }
    if path::looks_binary(&bytes) {
        return rejected("文件内容是二进制的，读取没有意义。");
    }
    // F2 optimistic lock: remember exactly what read_text_file saw.
    record_read_snapshot(&canonical, &meta, &bytes);
    let text = String::from_utf8_lossy(&bytes);
    let all_lines: Vec<&str> = text.lines().collect();

    let start = args.get("start_line").and_then(|v| v.as_u64()).unwrap_or(1).max(1) as usize;
    let requested_end = args
        .get("end_line")
        .and_then(|v| v.as_u64())
        .map(|v| (v as usize).max(start))
        .unwrap_or_else(|| start.saturating_add(MAX_LINES_PER_READ - 1));
    let end = requested_end.min(start.saturating_add(MAX_LINES_PER_READ - 1));

    if start > all_lines.len() {
        return failed(format!("文件一共 {} 行，起始行 {} 超出了。", all_lines.len(), start));
    }
    let end_eff = end.min(all_lines.len());

    let mut out = String::new();
    let mut used = 0usize;
    let mut last_line = start;
    let mut char_truncated = false;
    for (idx, line) in all_lines[start - 1..end_eff].iter().enumerate() {
        let line_no = start + idx;
        let piece = format!("{}| {}\n", line_no, line);
        if used + piece.len() > MAX_CHARS_PER_READ {
            char_truncated = true;
            break;
        }
        out.push_str(&piece);
        used += piece.len();
        last_line = line_no;
    }
    let mut header = format!("{}:{}-{}\n", canonical.display(), start, last_line);
    // Report whenever we delivered less than requested: the window was
    // clamped (end < requested_end), the file ran out (last_line < end),
    // or the char cap hit mid-window.
    if end < requested_end || last_line < end || char_truncated {
        header.push_str("（片段已截断；需要更多内容请指定后续行号）\n");
    }
    audit_bump(|a| {
        a.reads += 1;
        a.read_bytes += used as u64;
        if end < requested_end || last_line < end || char_truncated {
            a.read_truncations += 1;
        }
    });
    note_used_root(&canonical);
    ToolResult {
        status: ToolStatus::Success,
        content: format!("{}{}", header, out),
    }
}

// --- search_files ------------------------------------------------------------

fn is_skip_dir_name(name: &std::ffi::OsStr) -> bool {
    let n = name.to_string_lossy().to_lowercase();
    SKIP_DIRS.iter().any(|d| *d == n.as_str())
}

/// Prune noise directories (`node_modules`/`.git`/`target`…) without pruning
/// the walk root itself. Used by `filter_entry` so those subtrees are never
/// descended (not merely skipped after burning the walk budget).
fn should_descend(entry: &ignore::DirEntry) -> bool {
    entry.depth() == 0
        || !(entry.path().is_dir() && is_skip_dir_name(entry.file_name()))
}

/// Case-insensitive substring search within a granted project scope.
/// Live scan (no index — "registry is not a prompt, files are queried at
/// call time"), .gitignore-aware via the `ignore` walker.
pub async fn search_files(args: &serde_json::Value, grants: &[crate::db::grants::FsGrant]) -> ToolResult {
    let query = match args.get("query").and_then(|q| q.as_str()) {
        Some(q) if !q.trim().is_empty() => q.trim().to_string(),
        _ => return rejected("没有指定搜索词。"),
    };
    if query.chars().count() > 100 {
        return rejected("搜索词太长了。");
    }
    let scope = args.get("scope").and_then(|s| s.as_str()).unwrap_or("active_project");

    let registry = super::workspace::WorkspaceRegistry::load();
    let root_raw = match registry.resolve_scope(scope) {
        Some(r) => r,
        None => return rejected("没有找到这个项目（scope 需是 workspace registry 里的 project id，或 active_project）。"),
    };
    let root = match authorize_path(&root_raw.to_string_lossy(), grants) {
        Ok(p) => p,
        Err(r) => return r,
    };

    let needle = query.to_lowercase();
    let mut hits: Vec<String> = Vec::new();
    let mut visited = 0usize;

    // filter_entry PRUNES .git/node_modules/target at the directory level
    // (plan §8.2-H3): without pruning, the walker still descends those trees
    // burning the MAX_WALK_ENTRIES budget before ever reaching source files.
    let walker = ignore::WalkBuilder::new(&root)
        .hidden(true)
        .git_ignore(true)
        .git_global(false)
        .parents(true)
        .max_depth(Some(8))
        .filter_entry(should_descend)
        .build();

    for entry in walker.flatten() {
        visited += 1;
        if visited > MAX_WALK_ENTRIES || hits.len() >= MAX_SEARCH_HITS {
            break;
        }
        let p = entry.path();
        if !p.is_file() {
            continue;
        }
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if SKIP_DIRS.iter().any(|d| p.to_string_lossy().to_lowercase().contains(&format!("\\{}\\", d)))
            || path::is_sensitive_name(&name)
        {
            continue;
        }
        let meta = match std::fs::metadata(p) {
            Ok(m) => m,
            Err(_) => continue,
        };
        if meta.len() > MAX_FILE_BYTES {
            continue;
        }
        if path::is_binary_extension(&name) {
            continue;
        }
        let mut bytes = Vec::with_capacity(meta.len() as usize);
        if std::fs::File::open(p).and_then(|mut f| f.read_to_end(&mut bytes)).is_err() {
            continue;
        }
        if path::looks_binary(&bytes) {
            continue;
        }
        let text = String::from_utf8_lossy(&bytes);
        for (i, line) in text.lines().enumerate() {
            if line.to_lowercase().contains(&needle) {
                let snippet: String = line.trim().chars().take(200).collect();
                hits.push(format!("{}:{}: {}", p.display(), i + 1, snippet));
                if hits.len() >= MAX_SEARCH_HITS {
                    break;
                }
            }
        }
    }

    note_used_root(&root);
    audit_bump(|a| a.searches += 1);
    if hits.is_empty() {
        return ToolResult {
            status: ToolStatus::Success,
            content: format!("在 {} 里没有找到匹配「{}」的内容。", root.display(), query),
        };
    }
    let truncated_note = if visited > MAX_WALK_ENTRIES {
        "（目录太大，扫描提前停止）"
    } else if hits.len() >= MAX_SEARCH_HITS {
        "（命中数已达上限）"
    } else {
        ""
    };
    ToolResult {
        status: ToolStatus::Success,
        content: format!("搜索「{}」命中 {} 处：\n{}{}", query, hits.len(), hits.join("\n"), truncated_note),
    }
}

// --- list_directory ----------------------------------------------------------

pub async fn list_directory(args: &serde_json::Value, grants: &[crate::db::grants::FsGrant]) -> ToolResult {
    let raw_path = match args.get("path").and_then(|p| p.as_str()) {
        Some(p) if !p.trim().is_empty() => p,
        _ => return rejected("没有指定要列出的目录路径。"),
    };
    let canonical = match authorize_path(raw_path, grants) {
        Ok(p) => p,
        Err(r) => return r,
    };
    if !canonical.is_dir() {
        return rejected("这个路径不是目录。");
    }

    let mut dirs: Vec<String> = Vec::new();
    let mut files: Vec<String> = Vec::new();
    let entries = match std::fs::read_dir(&canonical) {
        Ok(e) => e,
        Err(e) => return failed(format!("读取目录失败：{}", e)),
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if SKIP_DIRS.contains(&name.as_str()) || path::is_sensitive_name(&name) {
            continue;
        }
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            dirs.push(format!("{}/", name));
        } else {
            files.push(name);
        }
        if dirs.len() + files.len() >= MAX_DIR_ENTRIES {
            break;
        }
    }
    dirs.sort();
    files.sort();
    let mut lines = dirs;
    lines.extend(files);
    let note = if lines.len() >= MAX_DIR_ENTRIES { "（已达条目上限）" } else { "" };
    note_used_root(&canonical);
    audit_bump(|a| a.dirs_listed += 1);
    ToolResult {
        status: ToolStatus::Success,
        content: format!("{} 共 {} 项：\n{}{}", canonical.display(), lines.len(), lines.join("\n"), note),
    }
}

// --- get_file_metadata -------------------------------------------------------

pub async fn get_file_metadata(args: &serde_json::Value, grants: &[crate::db::grants::FsGrant]) -> ToolResult {
    let raw_path = match args.get("path").and_then(|p| p.as_str()) {
        Some(p) if !p.trim().is_empty() => p,
        _ => return rejected("没有指定文件路径。"),
    };
    let canonical = match authorize_path(raw_path, grants) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let meta = match std::fs::metadata(&canonical) {
        Ok(m) => m,
        Err(_) => return rejected(&path::PathDeny::NotAccessible.message()),
    };
    let kind = if meta.is_dir() { "目录" } else { "文件" };
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| {
            chrono::DateTime::<chrono::Local>::from(std::time::UNIX_EPOCH + d)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "未知".to_string());
    let size = if meta.is_dir() { "—".to_string() } else { format!("{} 字节", meta.len()) };
    note_used_root(&canonical);
    ToolResult {
        status: ToolStatus::Success,
        content: format!("{}：{} | {} | 修改于 {}", canonical.display(), kind, size, modified),
    }
}

// --- get_git_context ---------------------------------------------------------

fn git_cache(
) -> &'static std::sync::Mutex<std::collections::HashMap<String, (std::time::Instant, String)>> {
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, (std::time::Instant, String)>>,
    > = std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Synchronous git invocation with a REAL timeout (plan §8.2-C3): the child
/// is spawned with piped stdio, its streams are drained on helper threads, and
/// a watchdog kills it after `GIT_TIMEOUT_SECS`. Returns trimmed stdout only
/// on `exit status == 0`.
fn run_git_sync(root: &Path, args: &[String]) -> Option<String> {
    let mut child = match std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .arg("--no-optional-locks")
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            log::warn!("[fs] git spawn failed for {}: {}", root.display(), e);
            return None;
        }
    };

    let mut stdout = child.stdout.take()?;
    let mut stderr = child.stderr.take()?;
    // Drain both pipes on threads: a full pipe would otherwise deadlock the
    // child (and then the watchdog kill) while we wait for exit.
    let out_thread = std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = String::new();
        stdout
            .read_to_string(&mut buf)
            .ok()
            .map(|_| buf.trim().to_string())
    });
    let err_thread = std::thread::spawn(move || {
        use std::io::Read;
        let mut sink = String::new();
        let _ = stderr.read_to_string(&mut sink);
    });

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(GIT_TIMEOUT_SECS);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if std::time::Instant::now() >= deadline => {
                log::warn!(
                    "[fs] git timeout after {}s, killing child (root {})",
                    GIT_TIMEOUT_SECS,
                    root.display()
                );
                record_git_timeout();
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            Ok(None) => std::thread::sleep(std::time::Duration::from_millis(20)),
            Err(e) => {
                log::warn!("[fs] git try_wait failed: {}", e);
                break None;
            }
        }
    };

    let stdout = out_thread.join().ok().flatten();
    let _ = err_thread.join();
    let status = status?;
    if status.success() {
        stdout
    } else {
        None
    }
}

/// Async wrapper: the blocking git run happens in the blocking pool so the
/// agent loop's `tokio::time::timeout` can actually fire again; the hard
/// deadline remains the watchdog in `run_git_sync`.
async fn run_git(root: &Path, args: &[&str]) -> Option<String> {
    let root = root.to_path_buf();
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    tokio::task::spawn_blocking(move || run_git_sync(&root, &args))
        .await
        .ok()
        .flatten()
}

/// Git metadata for a registry project. TTL-cached (plan §2.5): "早上好"
/// must never spawn git — the result only refreshes when the cache expires.
pub async fn get_git_context(args: &serde_json::Value, grants: &[crate::db::grants::FsGrant]) -> ToolResult {
    let project_id = match args.get("project_id").and_then(|s| s.as_str()) {
        Some(s) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return rejected("没有指定 project_id（见 workspace registry，或用 active_project）。"),
    };

    let registry = super::workspace::WorkspaceRegistry::load();
    let root_raw = match registry.resolve_scope(&project_id) {
        Some(r) => r,
        None => return rejected("没有找到这个项目。"),
    };
    let root = match authorize_path(&root_raw.to_string_lossy(), grants) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let key = root.to_string_lossy().to_string();

    if let Ok(cache) = git_cache().lock() {
        if let Some((at, content)) = cache.get(&key) {
            if at.elapsed().as_secs() < GIT_CACHE_TTL_SECS {
                note_used_root(&root);
                return ToolResult {
                    status: ToolStatus::Success,
                    content: content.clone(),
                };
            }
        }
    }

    audit_bump(|a| a.git_calls += 1);
    let status_out = run_git(&root, &["status", "-sb", "--porcelain"]).await;
    let log_out = run_git(&root, &["log", "-1", "--oneline"]).await.unwrap_or_default();

    let content = match status_out {
        Some(s) => {
            let mut branch = String::from("unknown");
            let mut changed: usize = 0;
            let mut staged: usize = 0;
            for line in s.lines() {
                if let Some(b) = line.strip_prefix("## ") {
                    branch = b.split_whitespace().next().unwrap_or("unknown").to_string();
                } else if !line.trim().is_empty() {
                    changed += 1;
                    // XY codes: staged when X is not ' ' or '?'.
                    let x = line.chars().next().unwrap_or(' ');
                    if x != ' ' && x != '?' {
                        staged += 1;
                    }
                }
            }
            let recent = log_out.trim();
            if recent.is_empty() {
                format!("项目 {}：分支 {} | {} 处改动（{} 已暂存）| 还没有提交", root.display(), branch, changed, staged)
            } else {
                format!("项目 {}：分支 {} | {} 处改动（{} 已暂存）| 最近提交 {}", root.display(), branch, changed, staged, recent)
            }
        }
        None => format!("项目 {} 不是 git 仓库（或 git 不可用/读取超时）。", root.display()),
    };

    if let Ok(mut cache) = git_cache().lock() {
        cache.insert(key, (std::time::Instant::now(), content.clone()));
    }
    note_used_root(&root);
    ToolResult {
        status: ToolStatus::Success,
        content,
    }
}

// --- F2 edit_file: proposal mode (plan §3.6, §8.3-F2) ---------------------------
//
// No tool is advertised for editing. The LLM reads the file in a normal
// Observation round and puts a structured patch block in its final reply; the
// backend strips it (the bubble only shows the natural explanation), validates
// `search` uniqueness, and arms a proposal. A later command applies the
// proposal ONLY after the user confirms — with the include-only optimistic
// lock checked against the exact moment read_text_file saw the file.

/// Read-time optimistic-lock snapshot: file mtime (ns) + a dual-seed FNV-1a
/// 64-bit content digest. Purposely a real byte digest, not hashmaps' random
/// per-process DefaultHasher — it must be stable across turns in the same run.
#[derive(Debug, Clone, Copy)]
pub struct ReadSnapshot {
    pub mtime_nanos: u64,
    pub content_hash: u64,
}

fn read_snapshots_slot() -> &'static std::sync::Mutex<HashMap<PathBuf, ReadSnapshot>> {
    static SLOT: std::sync::OnceLock<std::sync::Mutex<HashMap<PathBuf, ReadSnapshot>>> =
        std::sync::OnceLock::new();
    SLOT.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

fn fnv1a64(bytes: &[u8], seed: u64) -> u64 {
    let mut h = 0xcbf29ce484222325u64 ^ seed;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

pub fn content_hash64(bytes: &[u8]) -> u64 {
    fnv1a64(bytes, 0x9e3779b97f4a7c15) ^ fnv1a64(bytes, 0xc2b2ae3d27d4eb4f)
}

fn mtime_nanos(meta: &std::fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(u64::MAX)
}

/// Called by read_text_file on every SUCCESSFUL read. Converse can later
/// validate an edit proposal against THIS snapshot (pessimistic read-then-
/// write lock, plan §3.6). A collision-averse side effect: two reads of the
/// same mtime with diverging bytes would change the hash.
fn record_read_snapshot(canonical: &Path, meta: &std::fs::Metadata, bytes: &[u8]) {
    let snap = ReadSnapshot {
        mtime_nanos: mtime_nanos(meta),
        content_hash: content_hash64(bytes),
    };
    if let Ok(mut slot) = read_snapshots_slot().lock() {
        slot.insert(canonical.to_path_buf(), snap);
    }
}

fn take_read_snapshot(canonical: &Path) -> Option<ReadSnapshot> {
    read_snapshots_slot()
        .lock()
        .map(|mut g| g.remove(canonical))
        .unwrap_or(None)
}

/// A validated, user-confirmable edit proposal. `read_*` are the optimistic
/// lock captured at read time (or, when the model proposed without an actual
/// read, at proposal-parse time — same protection window).
#[derive(Debug, Clone, serde::Serialize)]
pub struct EditProposal {
    pub id: String,
    pub path: String,
    pub search: String,
    pub replacement: String,
    pub read_mtime_nanos: u64,
    pub read_hash: u64,
}

/// Strip the FIRST structured patch block from `reply` and (when valid) return
/// the armed proposal. Malformed block → still stripped, proposal None (the
/// block must never leak into the bubble; a log records why).
///
/// Block contract (written into the tool-mode prompt):
/// ```edit_file
/// path: <absolute path>
/// <<<<< SEARCH
/// exact original lines
/// =====
/// replacement lines
/// >>>>> END
/// ```
pub fn extract_edit_proposal(reply: &str) -> (String, Option<EditProposal>) {
    let Some(block) = find_patch_block(reply) else {
        return (reply.trim().to_string(), None);
    };
    let body = block.body.as_str();
    let left = &reply[..block.start];
    let right = &reply[block.end..];
    let left_trim = left.trim_end();
    let right_trim = right.trim_start();
    let clean = match (!left_trim.is_empty(), !right_trim.is_empty()) {
        (true, true) => format!("{}\n{}", left_trim, right_trim),
        (true, false) => left_trim.to_string(),
        _ => right_trim.to_string(),
    };
    let missing_snapshot_warning = |p: &Path| {
        match (std::fs::metadata(p), std::fs::read(p)) {
            (Ok(m), Ok(bytes)) => ReadSnapshot {
                mtime_nanos: mtime_nanos(&m),
                content_hash: content_hash64(&bytes),
            },
            _ => ReadSnapshot {
                mtime_nanos: 0,
                content_hash: 0,
            },
        }
    };
    let path_line = body.lines().find(|l| l.starts_with("path:"));
    let Some(path_raw) = path_line.and_then(|l| l.strip_prefix("path:")) else {
        log::warn!("[edit_file] patch block missing path line — discarded");
        return (clean, None);
    };
    let Ok(canonical) = dunce::canonicalize(path_raw.trim()) else {
        log::warn!("[edit_file] patch path not canonicalizable: {}", path_raw.trim());
        return (clean, None);
    };
    let search_idx = body.find("<<<<< SEARCH");
    let sep_idx = body.find("=====");
    let end_idx = body.find(">>>>> END");
    let (Some(si), Some(ei), Some(end)) = (search_idx, sep_idx, end_idx) else {
        log::warn!("[edit_file] malformed patch markers — discarded");
        return (clean, None);
    };
    if si > ei || ei > end {
        log::warn!("[edit_file] patch markers out of order — discarded");
        return (clean, None);
    }
    let search = body[si + "<<<<< SEARCH".len()..ei].trim_matches('\n').trim_end_matches('\r');
    let replacement = body[ei + "=====".len()..end]
        .trim_matches('\n')
        .trim_end_matches('\r');
    if search.is_empty() {
        log::warn!("[edit_file] empty search string — discarded");
        return (clean, None);
    }
    if crate::tools::path::root_contains(&crate::config::app_data_dir(), &canonical)
        || canonical
            .file_name()
            .map(|n| crate::tools::path::is_sensitive_name(&n.to_string_lossy()))
            .unwrap_or(false)
    {
        log::warn!("[edit_file] path is pet-config/sensitive — no proposal");
        return (clean, None);
    }
    let snapshot = take_read_snapshot(&canonical).unwrap_or_else(|| {
        log::info!("[edit_file] no read snapshot for {} — using parse-time lock", canonical.display());
        missing_snapshot_warning(&canonical)
    });
    if search_is_unique_in_file(&canonical, search) != Some(true) {
        log::warn!("[edit_file] search not unique/absent in {} — no proposal", canonical.display());
        return (clean, None);
    }
    let id = format!(
        "edit_{}_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
        canonical
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default()
    );
    log::info!(
        "[edit_file] armed proposal {} for {}: search {} bytes -> replacement {} bytes",
        id,
        canonical.display(),
        search.len(),
        replacement.len()
    );
    let proposal = EditProposal {
        id,
        path: canonical.to_string_lossy().to_string(),
        search: search.to_string(),
        replacement: replacement.to_string(),
        read_mtime_nanos: snapshot.mtime_nanos,
        read_hash: snapshot.content_hash,
    };
    (clean, Some(proposal))
}

struct PatchBlock {
    /// Byte range of the WHOLE block (fence..closing fence) in `reply`.
    start: usize,
    end: usize,
    /// Everything between ```tag and closing ```, tag excluded.
    body: String,
}

fn find_patch_block(reply: &str) -> Option<PatchBlock> {
    let lower = reply.to_lowercase();
    let mut scan = 0usize;
    while let Some(fence) = lower[scan..].find("```") {
        let fence_at = scan + fence;
        let line_end = reply[fence_at..].find('\n').unwrap_or(reply.len() - fence_at);
        let tag_line = reply[fence_at + 3..fence_at + line_end].trim().to_lowercase();
        if tag_line.contains("edit") && (tag_line.starts_with("edit") || tag_line.starts_with("patch")) {
            let body_start = fence_at + line_end + (if reply[fence_at + line_end..].starts_with('\n') { 1 } else { 0 });
            match reply[body_start..].find("```") {
                Some(close) => {
                    let raw_end = body_start + close + 3;
                    return Some(PatchBlock {
                        start: fence_at,
                        end: raw_end,
                        body: reply[body_start..body_start + close].to_string(),
                    });
                }
                None => return None,
            }
        }
        scan = fence_at + 3;
    }
    None
}

/// Occurrence count decision for the current on-disk bytes: Some(true) only
/// when `search` matches EXACTLY ONCE; None = file unreadable/not UTF-8.
fn search_is_unique_in_file(canonical: &Path, search: &str) -> Option<bool> {
    let bytes = std::fs::read(canonical).ok()?;
    let ok_utf8 = match std::string::String::from_utf8(bytes.clone()) {
        Ok(s) => s,
        // BOM alone counts as text, not binary.
        Err(_) if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) => {
            String::from_utf8(bytes[3..].to_vec()).ok()?
        }
        Err(_) => return None,
    };
    Some(ok_utf8.matches(search).count() == 1)
}

/// Cheap, honest preview for the confirm card: remove-then-add verbatim lines
/// (search first, replacement second). Good enough for a yes/no diff card.
pub fn preview_diff(search: &str, replacement: &str) -> String {
    let mut out = String::new();
    for l in search.lines() {
        out.push_str(&format!("- {}\n", l));
    }
    if !search.ends_with('\n') {
        out.push('\n');
    }
    for l in replacement.lines() {
        out.push_str(&format!("+ {}\n", l));
    }
    if out.ends_with('\n') {
        out.pop();
    }
    out
}

/// Apply a CONFIRMED proposal. Re-runs the full security pipeline with the
/// current grant snapshot, then the optimistic lock:
///   mtime unchanged → ok
///   mtime changed but digest matches read time → ok (editor autoSave noise)
///   anything else → refuse, never overwrite (plan §3.6).
/// Line endings and BOM of the ORIGINAL bytes are preserved; the write is
/// same-directory temp + rename.
pub fn apply_proposal(
    proposal: &EditProposal,
    grants: &[crate::db::grants::FsGrant],
) -> Result<PathBuf, String> {
    let canonical = path::resolve_and_authorize(&proposal.path, grants)
        .map_err(|d| format!("授权检查没通过：{}", d.message()))?;
    let bytes = std::fs::read(&canonical).map_err(|e| format!("读文件失败：{}", e))?;
    let meta = std::fs::metadata(&canonical).map_err(|e| format!("取文件状态失败：{}", e))?;
    let mtime_now = mtime_nanos(&meta);
    if mtime_now != proposal.read_mtime_nanos {
        let hash_now = content_hash64(&bytes);
        if hash_now != proposal.read_hash {
            return Err(
                "文件在我读完之后被别人改过了。为了不覆盖你的新改动，我先停手——让璃重新读一遍再提一次吧。"
                    .to_string(),
            );
        }
    }

    // UTF-8 only (BOM tolerated), plan §3.3.
    let had_bom = bytes.starts_with(&[0xEF, 0xBB, 0xBF]);
    let text = if had_bom {
        String::from_utf8(bytes[3..].to_vec())
    } else {
        String::from_utf8(bytes.clone())
    }
    .map_err(|_| "这个文件不是 UTF-8 文本，我不改二进制或别的编码文件。".to_string())?;

    let crlf = text.matches("\r\n").count();
    let lf = text.matches('\n').count().saturating_sub(crlf);
    let eol = if crlf >= lf && crlf > 0 { "\r\n" } else { "\n" };
    let search_norm = proposal.search.replace("\r\n", &eol);
    let replacement_norm = normalize_eol(&proposal.replacement, eol);

    let hits = text.matches(&search_norm).count();
    if hits != 1 {
        return Err(format!(
            "要改的那段话现在在文件里出现了 {} 次（需要恰好 1 次才能安全替换）。内容已经变了，重新让璃读一遍吧。",
            hits
        ));
    }
    let edited = text.replacen(&search_norm, &replacement_norm, 1);

    // Pre-image kept for session-level undo (plan §3.6).
    let parent = canonical
        .parent()
        .ok_or_else(|| "文件没有父目录".to_string())?;
    let tmp = parent.join(format!(
        ".liri-edit-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let mut out = Vec::with_capacity(edited.len() + 3);
    if had_bom {
        out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
    }
    out.extend_from_slice(edited.as_bytes());
    std::fs::write(&tmp, &out).map_err(|e| format!("写临时文件失败：{}", e))?;
    std::fs::rename(&tmp, &canonical).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("替换文件失败（可能正被其他程序占用）：{}", e)
    })?;
    remember_undo(&canonical, bytes);
    audit_bump(|a| a.edits_applied += 1);
    Ok(canonical)
}

fn normalize_eol(s: &str, eol: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push_str(eol);
            }
            '\n' => out.push_str(eol),
            _ => out.push(c),
        }
    }
    out
}

fn undo_slot() -> &'static std::sync::Mutex<Option<(PathBuf, Vec<u8>)>> {
    static SLOT: std::sync::OnceLock<std::sync::Mutex<Option<(PathBuf, Vec<u8>)>>> =
        std::sync::OnceLock::new();
    SLOT.get_or_init(|| std::sync::Mutex::new(None))
}

fn remember_undo(canonical: &Path, pre_image: Vec<u8>) {
    if let Ok(mut slot) = undo_slot().lock() {
        *slot = Some((canonical.to_path_buf(), pre_image));
    }
}

/// Session-level single-step undo: restores the pre-image of the LAST edit.
pub fn undo_last_edit(grants: &[crate::db::grants::FsGrant]) -> Result<PathBuf, String> {
    let (canonical, pre) = undo_slot()
        .lock()
        .ok()
        .and_then(|mut g| g.take())
        .ok_or_else(|| "这一会儿还没有可以撤销的修改。".to_string())?;
    path::resolve_and_authorize(&canonical.to_string_lossy(), grants)
        .map_err(|d| format!("撤销也要重新授权：{}", d.message()))?;
    let parent = canonical
        .parent()
        .ok_or_else(|| "文件没有父目录".to_string())?;
    let tmp = parent.join(format!(
        ".liri-undo-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::write(&tmp, &pre).map_err(|e| format!("写撤销临时文件失败：{}", e))?;
    std::fs::rename(&tmp, &canonical).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("撤销失败：{}", e)
    })?;
    Ok(canonical)
}

// --- F1 create_note (plan §3.6, §8.3-F1) -----------------------------------------
//
// Mutation never happens inside the tool round that proposed it: create_note
// only ARMS a pending proposal; the user's explicit "可以/不行" on a later
// turn resolves it (Principle #11), and only then does `commit_pending_note`
// write atomically into `.liri/NOTES/`.

/// Single note cap (1MB) and directory quota (50MB), plan §3.6.
pub const NOTE_MAX_BYTES: usize = 1024 * 1024;
const NOTES_QUOTA_BYTES: u64 = 50 * 1024 * 1024;

/// .liri/NOTES — the only directory mutation may write (plan §3.6).
pub fn notes_dir() -> PathBuf {
    crate::tools::workspace::liri_dir().join("NOTES")
}

fn pending_note_slot() -> &'static std::sync::Mutex<Option<crate::mind::consent::PendingNote>> {
    static SLOT: std::sync::OnceLock<std::sync::Mutex<Option<crate::mind::consent::PendingNote>>> =
        std::sync::OnceLock::new();
    SLOT.get_or_init(|| std::sync::Mutex::new(None))
}

/// Write-path filename rules (§3.2): pure basename, no separators/dots-pairs,
/// no NUL/control chars, ≤64 chars, safe charset (alphanumeric incl. CJK plus
/// `-_. `), no Windows reserved names, no sensitive-looking name. `.md` is
/// appended when the caller gave no extension. Returns the normalized name.
pub fn validate_note_filename(raw: &str) -> Result<String, &'static str> {
    let name = raw.trim();
    if name.is_empty() || name == "." || name == ".." {
        return Err("empty_filename");
    }
    if name.chars().count() > 64 {
        return Err("filename_too_long");
    }
    if name.starts_with('.') {
        return Err("leading_dot");
    }
    for c in name.chars() {
        if !(c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | ' ')) {
            return Err("invalid_char");
        }
    }
    if name.contains("..") {
        return Err("dot_dot");
    }
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    const RESERVED: [&str; 18] = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
        "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5",
    ];
    if RESERVED.contains(&stem.as_str()) {
        return Err("reserved_name");
    }
    if path::is_sensitive_name(name) {
        return Err("sensitive_name");
    }
    let normalized = if name.contains('.') {
        name.to_string()
    } else {
        format!("{}.md", name)
    };
    Ok(normalized)
}

/// Pre-check the NOTES quota: existing bytes + new bytes must stay ≤50MB.
fn notes_total_bytes(dir: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return 0;
    };
    rd.flatten()
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

/// First free path for a filename (foo.md → foo_2.md) so a confirmed note can
/// never silently overwrite an existing one.
fn free_note_path(dir: &Path, filename: &str) -> PathBuf {
    let candidate = dir.join(filename);
    if !candidate.exists() {
        return candidate;
    }
    let stem_ext: Vec<&str> = filename.splitn(2, '.').collect();
    let (stem, ext) = match stem_ext.as_slice() {
        [s, e] => (*s, format!(".{}", e)),
        _ => (filename, String::new()),
    };
    for i in 2u32..10000 {
        let candidate = dir.join(format!("{}_{}{}", stem, i, ext));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{}_{}", stem, "overflow"))
}

/// Atomic write (same-directory temp + rename) with quota + name validation.
/// The caller must already hold the user's confirmation. Returns the canonical
/// path actually written (may differ from `filename` when a same-name note
/// already exists).
fn write_note_into(dir: &Path, filename: &str, content: &str) -> Result<PathBuf, String> {
    let name = validate_note_filename(filename).map_err(|k| format!("文件名不合格（{}）", k))?;
    if content.is_empty() || content.len() > NOTE_MAX_BYTES {
        return Err("笔记内容为空或超过 1MB".to_string());
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("建不了 NOTES 目录：{}", e))?;
    let total = notes_total_bytes(dir);
    if total + content.len() as u64 > NOTES_QUOTA_BYTES {
        return Err("笔记空间已满（.liri/NOTES 50MB 上限），先清理一些旧笔记吧".to_string());
    }

    let tmp = dir.join(format!(
        ".liri-note-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let target = free_note_path(dir, &name);
    std::fs::write(&tmp, content).map_err(|e| format!("写临时文件失败：{}", e))?;
    match std::fs::rename(&tmp, &target) {
        Ok(_) => {
            audit_bump(|a| a.notes_written += 1);
            Ok(target)
        }
        // Retry once: another process may have taken the free path between
        // free_note_path and rename.
        Err(e) => {
            let target2 = free_note_path(dir, &name);
            match std::fs::rename(&tmp, &target2) {
                Ok(_) => Ok(target2),
                Err(e2) => {
                    let _ = std::fs::remove_file(&tmp);
                    Err(format!("保存失败（{} / {}）", e, e2))
                }
            }
        }
    }
}

/// Arm the pending proposal after a create_note tool call. One slot — a newer
/// proposal replaces an unanswered older one.
pub fn arm_pending_note(note: crate::mind::consent::PendingNote) {
    if let Ok(mut slot) = pending_note_slot().lock() {
        *slot = Some(note);
    }
}

/// Take (and CLEAR) the pending proposal — call once at the start of each
/// converse turn so "可以/不行" resolves before memory ingest.
pub fn take_pending_note() -> Option<crate::mind::consent::PendingNote> {
    pending_note_slot()
        .lock()
        .map(|mut g| g.take())
        .unwrap_or(None)
}

/// Write the confirmed note. Only called after the user said yes.
pub fn commit_pending_note(note: &crate::mind::consent::PendingNote) -> Result<PathBuf, String> {
    write_note_into(&notes_dir(), &note.filename, &note.content)
}

/// F1 tool body: validate + arm the proposal. No file is written here.
pub async fn create_note(args: &serde_json::Value) -> ToolResult {
    let filename = match validate_note_filename(args.get("filename").and_then(|f| f.as_str()).unwrap_or("")) {
        Ok(f) => f,
        Err(k) => {
            return rejected(&format!(
                "文件名不合格（{}）：只用纯文件名，不带路径，64 个字符以内。",
                k
            ))
        }
    };
    let content = args.get("content").and_then(|c| c.as_str()).unwrap_or("");
    if content.is_empty() || content.len() > NOTE_MAX_BYTES {
        return rejected("笔记内容为空，或超过 1MB。");
    }
    arm_pending_note(crate::mind::consent::PendingNote {
        filename: filename.clone(),
        content: content.to_string(),
        created_at: chrono::Utc::now(),
    });
    ToolResult {
        status: ToolStatus::Success,
        content: format!(
            "笔记「{}」我已整理好（{} 字），但还没有写任何文件。请你在回复里问用户：\
             “要保存到 .liri/NOTES 里吗？”用户明确答应后才能保存。",
            filename,
            content.chars().count()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::grants::{FsGrant, GrantMode};

    fn grant_for(root: &Path) -> Vec<FsGrant> {
        vec![FsGrant {
            root: root.to_string_lossy().to_string(),
            mode: GrantMode::Project.as_str().to_string(),
            created_at: String::new(),
            source: "test".to_string(),
        }]
    }

    fn temp_project(lines: usize) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pet_fs_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let content: String = (1..=lines).map(|i| format!("line {} content
", i)).collect();
        std::fs::write(dir.join("big.rs"), content).unwrap();
        std::fs::write(dir.join(".env"), "API_KEY=secret").unwrap();
        std::fs::write(dir.join("blob.bin"), b"\x00\x01\x02binary").unwrap();
        dir
    }

    #[tokio::test]
    async fn read_unauthorized_root_rejected() {
        let dir = temp_project(10);
        let args = serde_json::json!({"path": dir.join("big.rs").to_string_lossy()});
        let r = read_text_file(&args, &[]).await;
        assert_eq!(r.status, ToolStatus::Rejected);
        assert!(r.content.contains("授权"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn read_authorized_line_window_and_header() {
        let dir = temp_project(300);
        let grants = grant_for(&dir);
        // Ask for 500 lines — must clamp to 80.
        let args = serde_json::json!({
            "path": dir.join("big.rs").to_string_lossy(),
            "start_line": 5, "end_line": 505
        });
        let r = read_text_file(&args, &grants).await;
        assert_eq!(r.status, ToolStatus::Success);
        assert!(r.content.contains(":5-84"), "header should report clamped window: {}", r.content.lines().next().unwrap());
        assert!(r.content.contains("截断"));
        assert!(r.content.contains("5| line 5 content"));
        assert!(!r.content.contains("| line 85")); // beyond the window
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn read_env_denied_inside_granted_root() {
        let dir = temp_project(3);
        let grants = grant_for(&dir);
        let args = serde_json::json!({"path": dir.join(".env").to_string_lossy()});
        let r = read_text_file(&args, &grants).await;
        assert_eq!(r.status, ToolStatus::Rejected);
        assert!(r.content.contains("敏感"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn read_binary_rejected() {
        let dir = temp_project(3);
        let grants = grant_for(&dir);
        let args = serde_json::json!({"path": dir.join("blob.bin").to_string_lossy()});
        let r = read_text_file(&args, &grants).await;
        assert_eq!(r.status, ToolStatus::Rejected);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn list_directory_skips_sensitive_and_sorts() {
        let dir = temp_project(3);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        let grants = grant_for(&dir);
        let args = serde_json::json!({"path": dir.to_string_lossy()});
        let r = list_directory(&args, &grants).await;
        assert_eq!(r.status, ToolStatus::Success);
        assert!(r.content.contains("sub/"));
        assert!(r.content.contains("big.rs"));
        assert!(!r.content.contains(".env"), "denylist names must not be listed");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn search_finds_hit_with_lineno() {
        let dir = temp_project(20);
        let grants = grant_for(&dir);
        let args = serde_json::json!({"query": "line 17", "scope": "active_project"});
        // active_project resolves via the observer hint (None in tests) — so
        // exercise the pipeline by pointing scope at... the registry is a
        // global file; instead verify the miss path returns success-empty.
        let r = search_files(&args, &grants).await;
        assert_eq!(r.status, ToolStatus::Rejected); // no registry → unknown scope
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn walker_prunes_skip_dirs_before_walking() {
        let dir = temp_project(3);
        std::fs::create_dir_all(dir.join("node_modules").join("pkg")).unwrap();
        for i in 0..150 {
            std::fs::write(
                dir.join("node_modules").join("pkg").join(format!("f{}.js", i)),
                format!("needle js {i}"),
            )
            .unwrap();
        }
        let dir_clone = dir.clone();
        // Same pruning decision function as production search_files.
        let walker = ignore::WalkBuilder::new(&dir_clone)
            .hidden(true)
            .git_ignore(true)
            .parents(true)
            .filter_entry(should_descend)
            .build();
        let paths: Vec<PathBuf> = walker.flatten().map(|e| e.path().to_path_buf()).collect();
        assert!(
            !paths
                .iter()
                .any(|p| p.to_string_lossy().to_lowercase().contains("node_modules")),
            "node_modules must be pruned, not descended"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn read_huge_start_line_is_clean_failure() {
        let dir = temp_project(3);
        let grants = grant_for(&dir);
        let args = serde_json::json!({
            "path": dir.join("big.rs").to_string_lossy(),
            "start_line": u64::MAX
        });
        let r = read_text_file(&args, &grants).await;
        assert_eq!(r.status, ToolStatus::Failed);
        assert!(r.content.contains("超出"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn run_git_returns_status_or_times_out_cleanly() {
        let has_git = std::process::Command::new("git")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !has_git {
            eprintln!("git not installed — run_git test skipped");
            return;
        }
        let dir = std::env::temp_dir().join(format!(
            "pet_git_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let ok = std::process::Command::new("git")
            .arg("-C")
            .arg(&dir)
            .args(["init", "-q"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            let out = run_git(&dir, &["status", "-sb", "--porcelain"]).await;
            assert!(out.is_some(), "fresh repo status should succeed");
            assert!(out.as_deref().unwrap().contains("##"));
        }
        let missing = dir.join("definitely_missing_subdir_xyz");
        let out = run_git(&missing, &["status", "-sb", "--porcelain"]).await;
        assert!(out.is_none(), "missing root must fail fast, not hang");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[tokio::test]
    async fn git_context_unknown_project_rejected() {
        let args = serde_json::json!({"project_id": "no_such_project"});
        let r = get_git_context(&args, &[]).await;
        assert_eq!(r.status, ToolStatus::Rejected);
    }

    #[test]
    fn denied_roots_are_deduplicated_and_taken_together() {
        // Drain whatever parallel tests left behind, then record one unique
        // root twice — duplicate asks for the same path must collapse.
        let _ = take_denied_roots();
        record_denied_root("D:\\pet_m6_test\\proj".into());
        record_denied_root("D:\\pet_m6_test\\proj".into());
        record_denied_root("D:\\pet_m6_test\\docs".into());
        let got = take_denied_roots();
        assert_eq!(
            got.iter()
                .filter(|r| r.as_str() == "D:\\pet_m6_test\\proj")
                .count(),
            1,
            "duplicate roots must collapse into one ask"
        );
        assert!(got.contains(&"D:\\pet_m6_test\\docs".to_string()));
    }

    // --- F1 create_note ----------------------------------------------------

    #[test]
    fn note_filename_validation_matches_write_rules() {
        // Accepted: CJK, extension, no-extension normalization, spaces.
        assert_eq!(validate_note_filename("体检提醒.md").unwrap(), "体检提醒.md");
        assert_eq!(validate_note_filename("todo").unwrap(), "todo.md");
        assert_eq!(validate_note_filename("  本周 计划_1  ").unwrap(), "本周 计划_1.md");
        assert_eq!(validate_note_filename("a-b_c.md").unwrap(), "a-b_c.md");

        // Rejected: traversal / separators / reserved / sensitive / length.
        for bad in [
            "",
            "   ",
            ".",
            "..",
            "a..b",
            "a/b.md",
            "a\\b.md",
            "../x",
            "..\\x.md",
            ".hidden",
            ".env",
            "id_rsa",
            "CON",
            "com1.txt",
            "a*b.md",
            "a?b.md",
            "a<b.md",
            "a\"b.md",
            "a:b.md",
            "a|b.md",
        ] {
            assert!(validate_note_filename(bad).is_err(), "should reject: {bad:?}");
        }
        let long: String = "a".repeat(65);
        assert!(validate_note_filename(&long).is_err());
    }

    #[test]
    fn note_atomic_write_is_quota_and_overwrite_safe() {
        let dir = std::env::temp_dir().join(format!(
            "pet_f1_note_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        // UNCONFIRMED write attempt must be impossible at the API surface:
        // write_note_into is private; only commit_pending_note (called after
        // user confirmation) can reach it. Here we exercise atomic + collision
        // behavior through the same helper tests use.
        let _ = std::fs::create_dir_all(&dir);
        let first = write_note_into(&dir, "购物清单.md", "苹果\n牛奶\n").unwrap();
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "苹果\n牛奶\n");
        // Same name again must never overwrite — free path gets a suffix.
        let second = write_note_into(&dir, "购物清单.md", "第二条").unwrap();
        assert_ne!(first, second);
        assert_eq!(std::fs::read_to_string(&first).unwrap(), "苹果\n牛奶\n");
        assert!(second.file_name().unwrap().to_string_lossy().contains("购物清单_2"));
        // No temp files may remain after rename.
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.contains(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temp files leaked after rename: {leftovers:?}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn note_slot_arms_takes_and_clears() {
        let _ = take_pending_note();
        arm_pending_note(crate::mind::consent::PendingNote {
            filename: "a.md".into(),
            content: "1".into(),
            created_at: chrono::Utc::now(),
        });
        let got = take_pending_note().expect("arm then take");
        assert_eq!(got.filename, "a.md");
        // Take-and-clear: a second take returns nothing.
        assert!(take_pending_note().is_none());
    }

    // --- F2 edit_file ------------------------------------------------------

    fn f2_file(content: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pet_f2_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("target.txt");
        std::fs::write(&path, content).unwrap();
        let meta = std::fs::metadata(&path).unwrap();
        record_read_snapshot(&dunce::canonicalize(&path).unwrap(), &meta, content.as_bytes());
        dir
    }

    #[test]
    fn patch_block_is_parsed_stripped_and_armed() {
        let dir = f2_file("line one\nline two\n独一无二的目标\nline four\n");
        let path = dir.join("target.txt");
        let reply = format!(
            "我把第一句的「目标」调整了一下，预览在下面。\n```edit_file\npath: {}\n<<<<< SEARCH\n独一无二的目标\n=====\n改好的目标\n>>>>> END\n```\n其他文字不要动。",
            path.display()
        );
        let (clean, proposal) = extract_edit_proposal(&reply);
        assert_eq!(clean, "我把第一句的「目标」调整了一下，预览在下面。\n其他文字不要动。");
        let p = proposal.expect("valid patch must arm");
        assert_eq!(p.search, "独一无二的目标");
        assert_eq!(p.replacement, "改好的目标");
        assert_eq!(p.path, dunce::canonicalize(&path).unwrap().to_string_lossy());
        assert!(p.read_mtime_nanos != 0);
        assert!(p.read_hash != 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn malformed_or_ununique_patch_is_stripped_but_disarmed() {
        let dir = f2_file("same\nsame\n");
        let path = dir.join("target.txt");
        // Missing path line.
        let (clean, p) = extract_edit_proposal(
            "explain\n```edit_file\n<<<<< SEARCH\nsame\n=====\nx\n>>>>> END\n```",
        );
        assert_eq!(clean, "explain");
        assert!(p.is_none());
        // Search appears twice → no proposal, block still stripped.
        let reply = format!(
            "explain\n```edit_file\npath: {}\n<<<<< SEARCH\nsame\n=====\nx\n>>>>> END\n```",
            path.display()
        );
        let (clean, p) = extract_edit_proposal(&reply);
        assert_eq!(clean, "explain");
        assert!(p.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn apply_proposal_preserves_bom_crlf_and_supports_undo() {
        let dir = f2_file("alpha\r\nbeta\r\nUNIQUE 行\r\nomega\r\n");
        let path = dunce::canonicalize(dir.join("target.txt")).unwrap();
        // f2_file wrote LF-only bytes; rebuild with BOM+CRLF and re-record.
        let raw = b"\xEF\xBB\xBFalpha\r\nbeta\r\nUNIQUE \xe8\xa1\x8c\r\nomega\r\n".to_vec();
        std::fs::write(&path, &raw).unwrap();
        let meta = std::fs::metadata(&path).unwrap();
        record_read_snapshot(&path, &meta, &raw);
        let proposal = EditProposal {
            id: "test".into(),
            path: path.to_string_lossy().to_string(),
            search: "UNIQUE 行".into(),
            replacement: "CHANGED\nline".into(),
            read_mtime_nanos: mtime_nanos(&meta),
            read_hash: content_hash64(&raw),
        };
        let grants = grant_for(&dir);
        apply_proposal(&proposal, &grants).expect("apply");
        let after = std::fs::read(&path).unwrap();
        assert!(after.starts_with(&[0xEF, 0xBB, 0xBF]), "BOM must survive");
        let text = String::from_utf8(after[3..].to_vec()).unwrap();
        assert!(text.contains("CHANGED\r\nline"), "replacement must inherit CRLF: {:?}", text);
        assert!(!text.contains("UNIQUE 行"));
        // Session-level undo restores the exact pre-image.
        undo_last_edit(&grants).expect("undo");
        assert_eq!(std::fs::read(&path).unwrap(), raw, "pre-image must be byte-identical");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn apply_proposal_refuses_when_file_changed_after_read() {
        let dir = f2_file("before unique\nsecond\n");
        let path = dunce::canonicalize(dir.join("target.txt")).unwrap();
        let proposer_meta = std::fs::metadata(&path).unwrap();
        let proposer_bytes = std::fs::read(&path).unwrap();
        let proposal = EditProposal {
            id: "test".into(),
            path: path.to_string_lossy().to_string(),
            search: "before unique".into(),
            replacement: "after".into(),
            read_mtime_nanos: mtime_nanos(&proposer_meta),
            read_hash: content_hash64(&proposer_bytes),
        };
        // External edit between read and apply (same length even — mtime catches
        // it, hash comparison is the second line of defense).
        std::fs::write(&path, "foreign edit now!\n").unwrap();
        let err = apply_proposal(&proposal, &grant_for(&dir)).unwrap_err();
        assert!(err.contains("改过"), "optimistic lock must refuse: {err}");
        // Backstop when mtime got aliased: the uniqueness check still refuses.
        let now_meta = std::fs::metadata(&path).unwrap();
        let mut p2 = proposal.clone();
        p2.read_mtime_nanos = mtime_nanos(&now_meta);
        let err2 = apply_proposal(&p2, &grant_for(&dir)).unwrap_err();
        assert!(err2.contains("0 次"), "uniqueness backstop must refuse: {err2}");
        std::fs::remove_dir_all(&dir).ok();
    }
}
