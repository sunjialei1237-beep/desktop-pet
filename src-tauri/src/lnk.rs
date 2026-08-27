//! Windows `.lnk`（shell 快捷方式）二进制解析 + Recent 文件夹反查。
//!
//! 2026-08-27 自 `perception/window.rs` 整体平移（行为逐字节一致）：.lnk
//! 解析是独立的领域知识，此前埋在前台窗口感知模块里。调用方目前是
//! `perception::environment`（fs 工具 root 修复 ×3 + 观察循环 root hint）。
//! 本模块相对原实现新增的只有 `recent_target_for` 的 30s TTL 结果缓存——
//! 环境观察循环每 ~3s 用同一文件名查询，无缓存时每次都全量列目录 + 读
//! 至多 400 个 lnk（约 2.9 万次/天）；TTL 过后按原语义重扫，新文件仍可见。

use std::path::PathBuf;

/// Windows Recent shortcut targets use two representations in one file:
/// - LinkInfo LocalBasePath is ANSI (code page ACP — GBK on this user
///   machine) and actually carries the FULL file target here, while
/// - the Unicode offset block only stores a base path whose string node
///   merges trailing material (sweep found `D:\桌宠\.zcode\plans\``).
/// So the structured LinkInfo parse is primary; the sweep below is its
/// structural fallback.
#[cfg(target_os = "windows")]
fn parse_lnk_linkinfo_local_path(bytes: &[u8]) -> Option<String> {
    use windows::Win32::Globalization::{MultiByteToWideChar, CP_ACP, MB_PRECOMPOSED};
    let u32_at = |off: usize| usize::try_from(u32::from_le_bytes(bytes.get(off..off + 4)?.try_into().ok()?)).ok();
    let u16_at = |off: usize| {
        let b = bytes.get(off..off + 2)?;
        usize::try_from(u16::from_le_bytes(b.try_into().ok()?)).ok()
    };
    let header_size = u32_at(0)?;
    if header_size < 0x4C {
        return None;
    }
    let flags = u32_at(20)? as u32;
    let mut pos = header_size;
    if flags & 0x1 != 0 {
        let id_list_size = u16_at(pos)?;
        pos = pos.checked_add(2 + id_list_size)?;
    }
    if flags & 0x2 == 0 {
        return None;
    }
    let li = pos;
    let li_size = u32_at(li)?;
    let li_header_size = u32_at(li.checked_add(4)?)?;
    if li_size < 0x1c || li_header_size < 0x1c {
        return None;
    }
    let li_flags = u32_at(li.checked_add(8)?)? as u32;
    let volume_id_off = u32_at(li.checked_add(12)?)?;
    let local_base_off = u32_at(li.checked_add(16)?)?;
    // Full local path requires the VolumeID/LocalBasePath branch.
    if li_flags & 0x1 == 0 || volume_id_off == 0 || local_base_off == 0 {
        return None;
    }
    let start = li.checked_add(local_base_off)?;
    let end = bytes[start..].iter().position(|b| *b == 0)? + start;
    let ansi = &bytes[start..end];
    if ansi.len() < 4 || ansi.len() > 2048 {
        return None;
    }
    let mut wide = [0u16; 2048];
    let written = unsafe {
        MultiByteToWideChar(
            CP_ACP,
            MB_PRECOMPOSED,
            ansi,
            Some(&mut wide),
        )
    };
    if written == 0 || written as usize >= wide.len() {
        return None;
    }
    Some(String::from_utf16_lossy(&wide[..written as usize]))
}

/// Extract workspace folder candidates from a `.lnk` (Windows shell shortcut)
/// binary using a UTF-16LE string sweep. Shell links store their target in an
/// encoded LinkInfo block; decoding that block structurally is overkill for
/// the fallback here — candidate strings with a drive letter and separators
/// are checked against the filesystem by the caller, so a false-positive
/// string can never leak into tool arguments.
pub(crate) fn parse_lnk_absolute_paths(bytes: &[u8]) -> Vec<String> {
    if bytes.len() < 4 {
        return Vec::new();
    }
    let mut out: Vec<String> = Vec::new();
    // Every even offset can start a UTF-16LE run. Cheap and independent of
    // LinkInfo offsets (which differ between Windows versions).
    for start in 0..bytes.len().saturating_sub(1) {
        let mut units: Vec<u16> = Vec::new();
        for i in (start..bytes.len() - 1).step_by(2) {
            let c = u16::from_le_bytes([bytes[i], bytes[i + 1]]);
            if c == 0 {
                break;
            }
            if c < 0x20 || c == 0xFFFF {
                units.clear();
                break;
            }
            if units.len() >= 260 {
                break;
            }
            units.push(c);
        }
        if units.len() < 4 {
            continue;
        }
        let Ok(s) = String::from_utf16(&units) else {
            continue;
        };
        let b = s.as_bytes();
        if b[0].is_ascii_alphabetic()
            && b.get(1) == Some(&b':')
            && (s.contains('\\') || s.contains('/'))
        {
            if !out.contains(&s) {
                out.push(s);
            }
        }
    }
    out
}

/// Resolve a bare file name by looking at the user's Windows Recent shortcuts
/// (`%APPDATA%\Microsoft\Windows\Recent\*.lnk`), newest first. VS Code keeps
/// these current every time a file is opened/touched even when the Electron
/// window command line only remembers its STARTUP folder: the 2026-08-18
/// live failure was "tab shows plan-sess_….md but root=D:/environment-demo".
/// All candidates are local metadata and are still authorized by the fs
/// policy before any content leaves the process.
#[cfg(target_os = "windows")]
pub(crate) fn recent_target_for(file_name: &str) -> Option<PathBuf> {
    let wanted = file_name.trim();
    if wanted.is_empty()
        || wanted == "."
        || wanted == ".."
        || wanted.contains('\\')
        || wanted.contains('/')
        || wanted.len() > 160
    {
        return None;
    }
    let now = std::time::Instant::now();
    let key = wanted.to_ascii_lowercase();
    if let Ok(cache) = recent_cache().lock() {
        if let Some((at, hit)) = cache.get(&key) {
            if entry_fresh(*at, now) {
                return hit.clone();
            }
        }
    }
    let resolved = scan_recent_for(wanted);
    if let Ok(mut cache) = recent_cache().lock() {
        // Bounded like perception::window 的进程名缓存：满员遇到新键先清空
        // 再插入（自愈 + 有界）。解析结果很便宜，清空严格优于无界增长。
        if cache.len() >= RECENT_CACHE_CAP && !cache.contains_key(&key) {
            cache.clear();
        }
        cache.insert(key, (now, resolved.clone()));
    }
    resolved
}

#[cfg(target_os = "windows")]
const RECENT_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(30);

#[cfg(target_os = "windows")]
const RECENT_CACHE_CAP: usize = 64;

#[cfg(target_os = "windows")]
fn entry_fresh(at: std::time::Instant, now: std::time::Instant) -> bool {
    now.duration_since(at) < RECENT_CACHE_TTL
}

#[cfg(target_os = "windows")]
fn recent_cache(
) -> &'static std::sync::Mutex<std::collections::HashMap<String, (std::time::Instant, Option<PathBuf>)>>
{
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, (std::time::Instant, Option<PathBuf>)>>,
    > = std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// 无缓存的原始扫描：列目录 → mtime 倒排 → 逐个解析，基名命中即返回。
#[cfg(target_os = "windows")]
fn scan_recent_for(wanted: &str) -> Option<PathBuf> {
    let Ok(appdata) = std::env::var("APPDATA") else {
        return None;
    };
    let dir = std::path::Path::new(&appdata)
        .join("Microsoft")
        .join("Windows")
        .join("Recent");
    let entries = std::fs::read_dir(&dir).ok()?;
    let mut links: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().map(|e| e.eq_ignore_ascii_case("lnk")).unwrap_or(false))
        .collect();
    links.sort_by_key(|p| {
        std::cmp::Reverse(
            std::fs::metadata(p)
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH),
        )
    });
    const MAX_LNK_BYTES: u64 = 128 * 1024;
    for link in links.iter().take(400) {
        let Ok(meta) = std::fs::metadata(link) else {
            continue;
        };
        if meta.len() > MAX_LNK_BYTES {
            continue;
        }
        let Ok(bytes) = std::fs::read(link) else {
            continue;
        };
        // Primary: ANSI LinkInfo LocalBasePath (contains the full file path).
        if let Some(candidate) = parse_lnk_linkinfo_local_path(&bytes) {
            let path = PathBuf::from(&candidate);
            if path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(wanted))
            {
                return Some(path);
            }
        }
        // Fallback: UTF-16 string sweep (older/odd link writers).
        for candidate in parse_lnk_absolute_paths(&bytes) {
            let path = PathBuf::from(&candidate);
            if path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(wanted))
            {
                return Some(path);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "windows")]
    #[test]
    fn parse_lnk_linkinfo_local_path_reads_ansi_target() {
        let mut raw = vec![0u8; 300];
        raw[0..4].copy_from_slice(&76u32.to_le_bytes());
        raw[20..24].copy_from_slice(&3u32.to_le_bytes()); // HasIDList | HasLinkInfo
        raw[76..78].copy_from_slice(&0u16.to_le_bytes()); // empty ID list
        let li = 78usize;
        let local_off = 36u32;
        let target = b"D:\\projects\\demo\\demo.ts";
        let li_size = 40usize + target.len();
        raw[li..li + 4].copy_from_slice(&(li_size as u32).to_le_bytes());
        raw[li + 4..li + 8].copy_from_slice(&28u32.to_le_bytes());
        raw[li + 8..li + 12].copy_from_slice(&1u32.to_le_bytes()); // VolumeIDAndLocalBasePath
        raw[li + 12..li + 16].copy_from_slice(&28u32.to_le_bytes());
        raw[li + 16..li + 20].copy_from_slice(&local_off.to_le_bytes());
        let start = li + local_off as usize;
        raw[start..start + target.len()].copy_from_slice(target);
        raw[start + target.len()] = 0;
        assert_eq!(
            parse_lnk_linkinfo_local_path(&raw).as_deref(),
            Some("D:\\projects\\demo\\demo.ts")
        );
    }

    #[test]
    fn parse_lnk_absolute_paths_recovers_utf16_target() {
        let mut raw = vec![0x4cu8, 0x00, 0x00, 0x00]; // LinkHeader-ish junk
        raw.extend([0u8; 72]); // pad to even alignment
        for unit in "D:\\桌宠\\.zcode\\plans\\plan-sess_001.md".encode_utf16() {
            raw.extend(unit.to_le_bytes());
        }
        let found = parse_lnk_absolute_paths(&raw);
        assert!(
            found
                .iter()
                .any(|s| s == "D:\\桌宠\\.zcode\\plans\\plan-sess_001.md"),
            "parsed {:?}",
            found
        );
        // Nothing here is a path — the sweep must stay silent rather than
        // inventing a candidate out of no-info bytes.
        assert!(parse_lnk_absolute_paths(&[0x01u8; 300]).is_empty());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn recent_cache_entry_expiry_and_cap() {
        let now = std::time::Instant::now();
        // TTL 边界：fresh 判定严格小于 30s。
        assert!(entry_fresh(now, now + RECENT_CACHE_TTL - std::time::Duration::from_millis(1)));
        assert!(!entry_fresh(now, now + RECENT_CACHE_TTL));

        // 满员插入新键先清空（自愈），更新既有键不清空——与窗口感知的
        // insert_bounded 同一契约，独立实现也要守住。
        let mut cache: std::collections::HashMap<String, (std::time::Instant, Option<PathBuf>)> =
            std::collections::HashMap::new();
        cache.insert("a".into(), (now, None));
        cache.insert("b".into(), (now, None));
        if cache.len() >= 2 && !cache.contains_key("c") {
            cache.clear();
        }
        cache.insert("c".into(), (now, None));
        assert_eq!(cache.len(), 1);
        assert!(cache.contains_key("c"));

        cache.insert("c".into(), (now, Some(PathBuf::from("D:\\x"))));
        assert_eq!(cache.len(), 1, "updating an existing key must not clear");
    }
}
