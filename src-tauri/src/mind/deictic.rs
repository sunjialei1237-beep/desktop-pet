//! Deictic-time neutralization for memory anchors (proactive bubble
//! governance 2026-08-14).
//!
//! Memories are stored as they were said ("今天在找实习" / "用户今天读了一本好书").
//! Echoing those words verbatim days later makes the pet sound temporally
//! confused — "你说今天在找实习" when the memory is a week old. This module
//! strips relative time words ("今天/昨天/明天/最近/上周"…) from anchor text
//! before it reaches the LLM; the memory's absolute date is injected separately
//! as a bracketed reference ("这是 ta 8月13日 提到的事") so she still has a
//! correct sense of *when* without repeating the wrong word.
//!
//! Pure function over `&str` — no LLM, no DB (Architecture #1: Rust maintains
//! state; the LLM only voices).

/// Relative time words stripped from anchor text. Longest-first ordering does
/// not matter for correctness (simple substring replace), but keeping the list
/// explicit makes the behavior testable and auditable.
const DEICTIC_WORDS: &[&str] = &[
    // days
    "前天",
    "昨天",
    "今天",
    "明天",
    "后天",
    "今早",
    "今晚",
    "今儿",
    "明早",
    "明晚",
    "昨儿",
    // weeks / months / years
    "上个星期",
    "这个星期",
    "下个星期",
    "上周",
    "这周",
    "下周",
    "上个月",
    "这个月",
    "下个月",
    "去年",
    "今年",
    "明年",
    // vague recent/future
    "最近",
    "刚刚",
    "刚才",
    "前几天",
    "过几天",
    "前阵子",
    "这段时间",
    "这几天",
];

/// Strips relative time words from `text`.
///
/// ```text
/// "今天在找实习"                       -> "在找实习"
/// "用户昨天带宠物狗糯米去看了流浪狗"     -> "用户带宠物狗糯米去看了流浪狗"
/// "明早叫 ta 起床"                     -> "叫 ta 起床"
/// "在准备找实习"                       -> "在准备找实习"   (no deictic, untouched)
/// ```
pub fn neutralize_deictic(text: &str) -> String {
    let mut out = text.to_string();
    for w in DEICTIC_WORDS {
        out = out.replace(w, "");
    }
    // Collapse whitespace left by removals (Chinese usually has none, but be safe).
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Formats an RFC3339 timestamp or `YYYY-MM-DD` date as "M月D日" (no year) for
/// the bracketed time reference injected alongside a neutralized anchor.
/// Returns None when the string can't be parsed (the reference is then omitted —
/// the anchor is still safe, just less time-specific).
pub fn format_memory_date(iso: &str) -> Option<String> {
    use chrono::Datelike;
    let dt = if let Ok(d) = chrono::DateTime::parse_from_rfc3339(iso) {
        d.with_timezone(&chrono::Utc)
    } else if let Ok(nd) = chrono::NaiveDate::parse_from_str(iso, "%Y-%m-%d") {
        chrono::DateTime::<chrono::Utc>::from_naive_utc_and_offset(
            nd.and_hms_opt(0, 0, 0)?,
            chrono::Utc,
        )
    } else {
        return None;
    };
    Some(format!("{}月{}日", dt.month(), dt.day()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_day_words() {
        assert_eq!(neutralize_deictic("今天在找实习"), "在找实习");
        assert_eq!(neutralize_deictic("明天去面试"), "去面试");
        assert_eq!(neutralize_deictic("用户昨天带宠物狗糯米去看了流浪狗"), "用户带宠物狗糯米去看了流浪狗");
        assert_eq!(neutralize_deictic("明早叫 ta 起床"), "叫 ta 起床");
    }

    #[test]
    fn strips_week_month_vague_words() {
        assert_eq!(neutralize_deictic("上周开始健身"), "开始健身");
        assert_eq!(neutralize_deictic("最近在看某剧"), "在看某剧");
        assert_eq!(neutralize_deictic("刚才有点饿"), "有点饿");
    }

    #[test]
    fn passthrough_without_deictic() {
        assert_eq!(neutralize_deictic("在准备找实习"), "在准备找实习");
        assert_eq!(neutralize_deictic("喜欢篮球"), "喜欢篮球");
        assert_eq!(neutralize_deictic(""), "");
    }

    #[test]
    fn formats_dates() {
        assert_eq!(format_memory_date("2026-08-13T08:00:00+00:00").as_deref(), Some("8月13日"));
        assert_eq!(format_memory_date("2026-07-26").as_deref(), Some("7月26日"));
        assert_eq!(format_memory_date("not-a-date"), None);
    }
}
