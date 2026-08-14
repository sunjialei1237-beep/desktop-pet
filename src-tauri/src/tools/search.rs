//! Web search tool: `SearchProvider` trait + DuckDuckGo HTML provider.
//!
//! DuckDuckGo's `html.duckduckgo.com/html/` endpoint returns a static HTML
//! page we scrape with CSS selectors — no API key, rate-limit tolerant, and
//! swappable behind the trait (a future Bing/Brave provider just implements
//! `SearchProvider`). 铁律 #2: every snippet returned is UNTRUSTED — the agent
//! loop wraps results in `<tool_result untrusted>` and the system prompt tells
//! the LLM to treat them as external unverified content.

use async_trait::async_trait;
use scraper::{Html, Selector};

use super::policy::ToolStatus;
use super::ToolResult;

/// One search hit. `domain` is parsed from `url` so the LLM can cite sources.
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub domain: String,
    pub snippet: String,
    pub retrieved_at: String,
}

/// Provider-level failure modes. All map to a graceful, in-character tool
/// result ("今天好像搜不到呢") — never a crash.
pub enum SearchError {
    ProviderUnavailable,
    ParseFailed,
    RateLimited,
    Network,
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchError::ProviderUnavailable => write!(f, "搜索服务暂不可用"),
            SearchError::ParseFailed => write!(f, "搜索结果解析失败"),
            SearchError::RateLimited => write!(f, "搜索太频繁了，稍后再试"),
            SearchError::Network => write!(f, "网络连接失败"),
        }
    }
}

/// A pluggable search backend. The agent loop holds one provider; swapping
/// DuckDuckGo for another backend is a one-line change.
#[async_trait]
pub trait SearchProvider: Send + Sync {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, SearchError>;
}

/// DuckDuckGo HTML endpoint provider. No API key required.
/// Kept as a swappable `SearchProvider`; in mainland China DDG is CAPTCHA-
/// blocked, so `ToutiaoProvider` is the default (`search_web`). Construct it
/// explicitly to use DDG (e.g. behind a VPN).
#[allow(dead_code)]
pub struct DuckDuckGoProvider {
    client: reqwest::Client,
}

#[allow(dead_code)]
impl DuckDuckGoProvider {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
                 AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36",
            )
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client }
    }
}

impl Default for DuckDuckGoProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SearchProvider for DuckDuckGoProvider {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, SearchError> {
        // DDG rate-limits / challenges automated traffic (datacenter IPs often
        // get a 202 challenge page with zero results). Try POST first (canonical
        // html endpoint), then fall back to GET — the two hit slightly different
        // code paths, so a GET retry recovers some blocks. On a home network
        // (where the pet actually runs) DDG is generally permissive.
        if let Some(results) = self.try_fetch(true, query, limit).await? {
            return Ok(results);
        }
        log::debug!("[search] POST returned no results, retrying as GET");
        if let Some(results) = self.try_fetch(false, query, limit).await? {
            return Ok(results);
        }
        Err(SearchError::ParseFailed)
    }
}

impl DuckDuckGoProvider {
    #[allow(dead_code)]
    /// One fetch attempt (POST or GET). Returns `Ok(None)` when the page loaded
    /// but yielded zero results, so the caller can retry the other method; an
    /// `Err` means a hard failure (network / rate-limit / non-html) worth
    /// surfacing.
    async fn try_fetch(
        &self,
        post: bool,
        query: &str,
        limit: usize,
    ) -> Result<Option<Vec<SearchResult>>, SearchError> {
        let req = if post {
            self.client
                .post("https://html.duckduckgo.com/html/")
                .header("Origin", "https://html.duckduckgo.com")
                .header("Referer", "https://html.duckduckgo.com/")
                .header("Sec-Fetch-Site", "same-origin")
                .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
                .form(&[("q", query)])
        } else {
            self.client
                .get(format!(
                    "https://html.duckduckgo.com/html/?q={}",
                    url_encode_query(query)
                ))
                .header("Sec-Fetch-Mode", "navigate")
                .header("Accept-Language", "zh-CN,zh;q=0.9,en;q=0.8")
        };
        let resp = req.send().await.map_err(|e| {
            if e.is_timeout() {
                SearchError::Network
            } else {
                SearchError::Network
            }
        })?;

        let status = resp.status();
        if status.as_u16() == 429 {
            return Err(SearchError::RateLimited);
        }
        if !status.is_success() {
            return Err(SearchError::ProviderUnavailable);
        }
        // Defense layer: content-type must be HTML, not an error JSON / redirect page.
        let content_type = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();
        if !content_type.contains("html") {
            return Err(SearchError::ParseFailed);
        }

        let html = resp.text().await.map_err(|_| SearchError::Network)?;
        let results = parse_ddg_html(&html, limit);
        Ok(if results.is_empty() { None } else { Some(results) })
    }
}

/// Minimal URL-query encoder for the GET fallback (DDG expects %XX-encoded q).
#[allow(dead_code)]
fn url_encode_query(q: &str) -> String {
    q.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c.to_string()
            } else {
                // Encode each char's UTF-8 bytes (handles CJK correctly).
                c.to_string().into_bytes().iter().map(|b| format!("%{:02X}", b)).collect()
            }
        })
        .collect()
}

/// Parse DuckDuckGo's HTML result page. Selectors: `.result__a` (title+link),
/// `.result__snippet` (summary). URLs are DDG-redirect-wrapped
/// (`//duckduckgo.com/l/?uddg=<encoded>`) — `extract_real_url` unwraps them.
fn parse_ddg_html(html: &str, limit: usize) -> Vec<SearchResult> {
    let document = Html::parse_document(html);
    let link_sel = match Selector::parse(".result__a") {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    let snippet_sel = match Selector::parse(".result__snippet") {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let links: Vec<_> = document.select(&link_sel).collect();
    let snippets: Vec<_> = document.select(&snippet_sel).collect();
    let now = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string();

    let n = links.len().min(limit);
    (0..n)
        .map(|i| {
            let el = links[i];
            let title = el.text().collect::<String>().trim().to_string();
            let href = el.value().attr("href").unwrap_or("");
            let url = extract_real_url(href);
            let domain = extract_domain(&url);
            let snippet = snippets
                .get(i)
                .map(|s| s.text().collect::<String>().trim().to_string())
                .unwrap_or_default();
            SearchResult {
                title,
                url,
                domain,
                snippet,
                retrieved_at: now.clone(),
            }
        })
        .filter(|r| !r.title.is_empty() && !r.url.is_empty())
        .collect()
}

/// Unwrap a DuckDuckGo redirect URL to the real destination. DDG wraps links as
/// `//duckduckgo.com/l/?uddg=<percent-encoded>&rut=...`; we decode the `uddg`
/// param. Non-wrapped hrefs (rare) get a `https:` scheme prefix if bare.
fn extract_real_url(href: &str) -> String {
    if let Some(pos) = href.find("uddg=") {
        let rest = &href[pos + "uddg=".len()..];
        let end = rest.find('&').unwrap_or(rest.len());
        let encoded = &rest[..end];
        return url_decode(encoded);
    }
    if href.starts_with("//") {
        format!("https:{}", href)
    } else {
        href.to_string()
    }
}

/// Extract the host/domain from a URL (`https://www.example.com/path` → `www.example.com`).
fn extract_domain(url: &str) -> String {
    let no_scheme = url.split("://").nth(1).unwrap_or(url);
    let host = no_scheme.split('/').next().unwrap_or(no_scheme);
    host.to_string()
}

/// Minimal percent-decoding for URL params (%XX → byte, + → space). Collects
/// raw bytes then lossy-converts to String so multi-byte UTF-8 (CJK) decodes
/// correctly.
fn url_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(pair) = std::str::from_utf8(&b[i + 1..i + 3]) {
                if let Ok(byte) = u8::from_str_radix(pair, 16) {
                    out.push(byte);
                    i += 3;
                    continue;
                }
            }
        }
        if b[i] == b'+' {
            out.push(b' ');
        } else {
            out.push(b[i]);
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `search_web` tool entry point (called by `tools::execute`). Returns a
/// formatted text block of top-5 results — title/url/snippet per hit, capped
/// so the whole payload stays well under the 1600-token tool-result budget.
pub async fn search_web(args: &serde_json::Value) -> ToolResult {
    let query = args.get("query").and_then(|q| q.as_str()).unwrap_or("");
    if query.trim().is_empty() {
        return ToolResult {
            status: ToolStatus::Rejected,
            content: "搜索词为空。".to_string(),
        };
    }
    let provider = ToutiaoProvider::new();
    match provider.search(query, 5).await {
        Ok(results) => {
            let content = format_results(&results);
            ToolResult {
                status: ToolStatus::Success,
                content,
            }
        }
        Err(e) => {
            log::warn!("[tools/search] query={:?} failed: {}", query, e);
            ToolResult {
                status: ToolStatus::Failed,
                content: format!("搜索失败：{}", e),
            }
        }
    }
}

/// 头条搜索 provider（国内可用，无需 API key）。`so.toutiao.com/search?
/// pd=information` 返回 SSR JSON，含真实 title/abstract。DDG/Bing/百度 在
/// 国内被反爬/CAPTCHA，头条是目前最稳定的免费国内搜索源。
pub struct ToutiaoProvider {
    client: reqwest::Client,
}

impl ToutiaoProvider {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .user_agent(
                "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
                 AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36",
            )
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client }
    }
}

impl Default for ToutiaoProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SearchProvider for ToutiaoProvider {
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchResult>, SearchError> {
        let resp = self
            .client
            .get("https://so.toutiao.com/search")
            .query(&[("keyword", query), ("pd", "information"), ("dvpf", "pc")])
            .header("Accept-Language", "zh-CN,zh;q=0.9")
            .send()
            .await
            .map_err(|_| SearchError::Network)?;
        let status = resp.status();
        if status.as_u16() == 429 {
            return Err(SearchError::RateLimited);
        }
        if !status.is_success() {
            return Err(SearchError::ProviderUnavailable);
        }
        let html = resp.text().await.map_err(|_| SearchError::Network)?;
        let results = parse_toutiao(&html, limit);
        if results.is_empty() {
            return Err(SearchError::ParseFailed);
        }
        Ok(results)
    }
}

/// Parse toutiao SSR HTML: skim the embedded JSON for `"title"`/`"abstract"`
/// string values and pair them by index. Each result object carries both fields;
/// titles come in an em-highlighted + plain pair, so we clean + dedup.
fn parse_toutiao(html: &str, limit: usize) -> Vec<SearchResult> {
    let now = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%:z")
        .to_string();
    let titles = extract_field_values(html, "title");
    let abstracts = extract_field_values(html, "abstract");
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for (i, title) in titles.iter().enumerate() {
        let clean = clean_em_tags(title);
        let ccount = clean.chars().count();
        if ccount < 4 || seen.contains(&clean) {
            continue;
        }
        seen.insert(clean.clone());
        let snippet = abstracts.get(i).cloned().unwrap_or_default();
        out.push(SearchResult {
            title: clean,
            url: String::new(), // toutiao group URLs interleave with image URLs; omitted
            domain: "今日头条".to_string(),
            snippet,
            retrieved_at: now.clone(),
        });
        if out.len() >= limit {
            break;
        }
    }
    out
}

/// Strip `<em></em>` highlight tags toutiao injects around query terms.
fn clean_em_tags(s: &str) -> String {
    s.replace("<em>", "").replace("</em>", "")
}

/// Extract all `"field":"value"` string values from an HTML/JSON blob, decoding
/// JSON escapes (\uXXXX, \", \\, \n, \/). Manual scan — no regex crate needed,
/// and we don't have to locate toutiao's exact SSR JSON container; we just skim
/// the whole response for the string fields we care about.
fn extract_field_values(html: &str, field: &str) -> Vec<String> {
    let needle = format!("\"{}\":\"", field);
    let bytes = html.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while let Some(rel) = html[i..].find(&needle) {
        let start = i + rel + needle.len();
        let mut j = start;
        let mut val: Vec<u8> = Vec::new();
        let mut escaped = false;
        while j < bytes.len() {
            let b = bytes[j];
            if escaped {
                match b {
                    b'"' => val.push(b'"'),
                    b'\\' => val.push(b'\\'),
                    b'/' => val.push(b'/'),
                    b'n' => val.push(b'\n'),
                    b't' => val.push(b'\t'),
                    b'u' if j + 4 < bytes.len() => {
                        let hex = std::str::from_utf8(&bytes[j + 1..j + 5]).unwrap_or("0020");
                        if let Ok(cp) = u32::from_str_radix(hex, 16) {
                            if let Some(ch) = char::from_u32(cp) {
                                let mut buf = [0u8; 4];
                                val.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
                            }
                        }
                        j += 4;
                    }
                    _ => val.push(b),
                }
                escaped = false;
                j += 1;
            } else if b == b'\\' {
                escaped = true;
                j += 1;
            } else if b == b'"' {
                break;
            } else {
                val.push(b);
                j += 1;
            }
        }
        out.push(String::from_utf8_lossy(&val).into_owned());
        i = start;
    }
    out
}

fn format_results(results: &[SearchResult]) -> String {
    let mut out = String::new();
    // All hits share one retrieval timestamp (a single search round).
    if let Some(r) = results.first() {
        out.push_str(&format!("（检索时间：{}）\n\n", r.retrieved_at));
    }
    for (i, r) in results.iter().enumerate().take(5) {
        out.push_str(&format!(
            "{}. {}\n   来源：{}\n   {}\n\n",
            i + 1,
            truncate(&r.title, 120),
            r.domain,
            truncate(&r.snippet, 300),
        ));
    }
    if out.is_empty() {
        "没有找到相关结果。".to_string()
    } else {
        out
    }
}

fn truncate(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let trimmed: String = s.chars().take(max_chars).collect();
    format!("{}…", trimmed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_real_url_wrapped() {
        let href = "//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Fpath&rut=abc";
        assert_eq!(extract_real_url(href), "https://example.com/path");
    }

    #[test]
    fn test_extract_real_url_wrapped_cjk() {
        // CJK in the encoded URL decodes correctly (multi-byte UTF-8).
        let href = "//duckduckgo.com/l/?uddg=https%3A%2F%2Fzh.wikipedia.org%2Fwiki%2F%E4%BA%BA%E5%B7%A5%E6%99%BA%E8%83%BD";
        assert_eq!(extract_real_url(href), "https://zh.wikipedia.org/wiki/人工智能");
    }

    #[test]
    fn test_extract_real_url_bare() {
        assert_eq!(extract_real_url("//example.com/foo"), "https://example.com/foo");
        assert_eq!(extract_real_url("https://x.com"), "https://x.com");
    }

    #[test]
    fn test_extract_domain() {
        assert_eq!(extract_domain("https://www.example.com/path?q=1"), "www.example.com");
        assert_eq!(extract_domain("https://news.site.org"), "news.site.org");
    }

    #[test]
    fn test_url_decode() {
        assert_eq!(url_decode("hello%20world"), "hello world");
        assert_eq!(url_decode("a+b"), "a b");
        assert_eq!(url_decode("%E4%BD%A0%E5%A5%BD"), "你好");
    }

    #[test]
    fn test_parse_ddg_html_extracts_results() {
        // A minimal DDG-style HTML fragment with two results.
        let html = r#"
        <html><body>
          <div class="result">
            <h2 class="result__title">
              <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fopenai.com&rut=x">OpenAI</a>
            </h2>
            <a class="result__snippet">AI research lab.</a>
          </div>
          <div class="result">
            <h2 class="result__title">
              <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fanthropic.com&rut=y">Anthropic</a>
            </h2>
            <a class="result__snippet">AI safety company.</a>
          </div>
        </body></html>"#;
        let results = parse_ddg_html(html, 5);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].title, "OpenAI");
        assert_eq!(results[0].url, "https://openai.com");
        assert_eq!(results[0].domain, "openai.com");
        assert_eq!(results[0].snippet, "AI research lab.");
        assert_eq!(results[1].title, "Anthropic");
    }

    #[test]
    fn test_parse_ddg_html_empty() {
        let results = parse_ddg_html("<html><body>no results</body></html>", 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_ddg_html_respects_limit() {
        let mut html = String::from("<html><body>");
        for i in 0..10 {
            html.push_str(&format!(
                r#"<div class="result"><a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fx{}.com">Item {}</a></div>"#,
                i, i
            ));
        }
        html.push_str("</body></html>");
        let results = parse_ddg_html(&html, 3);
        assert_eq!(results.len(), 3);
    }

    #[test]
    fn test_format_results() {
        let results = vec![SearchResult {
            title: "Test Title".to_string(),
            url: "https://example.com".to_string(),
            domain: "example.com".to_string(),
            snippet: "A snippet.".to_string(),
            retrieved_at: "now".to_string(),
        }];
        let out = format_results(&results);
        assert!(out.contains("Test Title"));
        assert!(out.contains("example.com"));
        assert!(out.contains("A snippet."));
    }

    #[test]
    fn test_extract_field_values_basic() {
        let html = r#"{"title":"hello","abstract":"world","title":"foo"}"#;
        assert_eq!(extract_field_values(html, "title"), vec!["hello", "foo"]);
        assert_eq!(extract_field_values(html, "abstract"), vec!["world"]);
    }

    #[test]
    fn test_extract_field_values_unicode_escape() {
        // \u003cem\u003e = <em>; \u65b0 = 新
        let html = r#"{"title":"\u003cem\u003eAI\u003c/em\u003e\u65b0\u95fb"}"#;
        let titles = extract_field_values(html, "title");
        assert_eq!(titles[0], "<em>AI</em>新闻");
    }

    #[test]
    fn test_clean_em_tags() {
        assert_eq!(clean_em_tags("<em>AI</em>新闻"), "AI新闻");
    }

    #[test]
    fn test_parse_toutiao_dedup_and_clean() {
        // The em-highlighted title and the plain title both appear; after
        // cleaning they're equal and must dedup.
        let html = concat!(
            r#"{"title":"AI新闻","abstract":"摘要一"}"#,
            r#"{"title":"\u003cem\u003eAI\u003c/em\u003e新闻","abstract":"dup"}"#,
            r#"{"title":"科技动态","abstract":"摘要二"}"#,
        );
        let results = parse_toutiao(html, 5);
        assert!(results.iter().any(|r| r.title == "AI新闻"));
        assert!(results.iter().any(|r| r.title == "科技动态"));
        assert!(results.len() <= 2, "em+plain should dedup, got {}", results.len());
    }
}
