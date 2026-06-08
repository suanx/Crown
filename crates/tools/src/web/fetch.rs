//! Web page fetching with HTML extraction and caching.

use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use lru::LruCache;
use parking_lot::Mutex;
use reqwest::Client;
use scraper::{ElementRef, Html, Selector};
use url::Url;

/// Maximum content length to fetch (10 MB).
const MAX_CONTENT_LENGTH: usize = 10 * 1024 * 1024;
/// Maximum output text length returned to model (80K chars).
const MAX_OUTPUT_LENGTH: usize = 80_000;
/// Maximum URL length.
const MAX_URL_LENGTH: usize = 2000;
/// Cache TTL (15 minutes).
const CACHE_TTL: Duration = Duration::from_secs(15 * 60);
/// Cache max entries.
const CACHE_MAX_ENTRIES: usize = 32;
/// Fetch timeout.
const FETCH_TIMEOUT: Duration = Duration::from_secs(60);
/// Max same-host redirects to follow.
const MAX_REDIRECTS: u8 = 10;

/// Cached fetch result.
struct CacheEntry {
    content: String,
    fetched_at: Instant,
    _status: u16,
}

/// Shared fetch cache (LRU + TTL).
pub struct FetchCache {
    inner: Mutex<LruCache<String, CacheEntry>>,
}

impl FetchCache {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(LruCache::new(NonZeroUsize::new(CACHE_MAX_ENTRIES).unwrap())),
        }
    }

    fn get(&self, url: &str) -> Option<String> {
        let mut cache = self.inner.lock();
        if let Some(entry) = cache.get(url) {
            if entry.fetched_at.elapsed() < CACHE_TTL {
                return Some(entry.content.clone());
            }
            // Expired — remove
        }
        cache.pop(url);
        None
    }

    fn put(&self, url: String, content: String, status: u16) {
        let mut cache = self.inner.lock();
        cache.put(
            url,
            CacheEntry {
                content,
                fetched_at: Instant::now(),
                _status: status,
            },
        );
    }
}

impl Default for FetchCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Fetch result returned to tool.
pub struct FetchResult {
    pub content: String,
    pub status: u16,
    pub url: String,
    pub content_type: String,
}

/// Validate a URL before fetching.
pub fn validate_url(url_str: &str) -> Result<Url, String> {
    if url_str.len() > MAX_URL_LENGTH {
        return Err(format!(
            "URL exceeds maximum length of {} characters",
            MAX_URL_LENGTH
        ));
    }

    let parsed = Url::parse(url_str).map_err(|e| format!("Invalid URL: {}", e))?;

    // Only allow http/https
    match parsed.scheme() {
        "http" | "https" => {}
        other => return Err(format!("Unsupported URL scheme: {}", other)),
    }

    // Block URLs with credentials
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("URLs with embedded credentials are not allowed".into());
    }

    // Block private/internal IPs
    if let Some(host) = parsed.host_str() {
        if is_private_host(host) {
            return Err(format!(
                "Access to private/internal host '{}' is not allowed",
                host
            ));
        }
    }

    Ok(parsed)
}

/// Check if a host is private/internal/otherwise unsafe to fetch (SSRF guard).
///
/// Strategy: if the host is an IP literal (incl. integer/hex/octal-encoded
/// forms and IPv4-mapped IPv6), classify the actual `IpAddr` with std's range
/// checks (loopback / private / link-local / unspecified / etc.). Otherwise
/// apply name-based heuristics for localhost and internal TLDs.
///
/// Note: this validates the URL's literal host. Full DNS-rebinding protection
/// would require re-checking the resolved socket IP at connect time; that is a
/// deeper change tracked separately. This closes the large literal-host hole
/// (cloud metadata 169.254.169.254, all of 127/8, link-local, IPv6, and
/// integer-encoded IPs) that the old prefix check missed.
fn is_private_host(host: &str) -> bool {
    let mut lower = host.trim().to_lowercase();
    if lower.is_empty() {
        return true;
    }
    // IPv6 literals arrive bracketed from some URL parsers (e.g. "[::1]").
    if lower.starts_with('[') && lower.ends_with(']') {
        lower = lower[1..lower.len() - 1].to_string();
    }

    // Name-based: localhost + internal TLDs.
    if lower == "localhost" || lower.ends_with(".localhost") {
        return true;
    }
    if lower.ends_with(".local") || lower.ends_with(".internal") {
        return true;
    }

    // IP literal in any common encoding → classify the actual address.
    if let Some(ip) = parse_host_ip(&lower) {
        return ip_is_unsafe(&ip);
    }

    // A real domain name reaches here and is treated as public.
    false
}

/// Parse a URL host into an [`IpAddr`], covering dotted IPv4/IPv6,
/// IPv4-mapped IPv6, and integer / hex / octal-encoded IPv4 forms. Returns
/// `None` for real domain names.
fn parse_host_ip(host: &str) -> Option<std::net::IpAddr> {
    use std::net::{IpAddr, Ipv4Addr};

    // Plain dotted IPv4 / IPv6 (url crate yields IPv6 without brackets).
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Some(ip);
    }

    // Integer-encoded IPv4: hex (0x7f000001), octal (017700000001), decimal
    // (2130706433) — all resolve to a single u32 address.
    let as_u32 = if let Some(hex) = host.strip_prefix("0x") {
        u32::from_str_radix(hex, 16).ok()
    } else if host.len() > 1 && host.starts_with('0') && host.bytes().all(|b| b.is_ascii_digit()) {
        u32::from_str_radix(host, 8).ok()
    } else if !host.is_empty() && host.bytes().all(|b| b.is_ascii_digit()) {
        host.parse::<u32>().ok()
    } else {
        None
    };
    as_u32.map(|n| IpAddr::V4(Ipv4Addr::from(n)))
}

/// Classify an [`IpAddr`] as unsafe to fetch (loopback, private, link-local,
/// unspecified, broadcast, documentation, and non-global IPv6 ranges).
fn ip_is_unsafe(ip: &std::net::IpAddr) -> bool {
    use std::net::IpAddr;
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()            // 127.0.0.0/8
                || v4.is_private()      // 10/8, 172.16/12, 192.168/16
                || v4.is_link_local()   // 169.254.0.0/16 (incl. cloud metadata)
                || v4.is_unspecified()  // 0.0.0.0
                || v4.is_broadcast()
                || v4.is_documentation()
                || v4.octets()[0] == 0 // 0.0.0.0/8
        }
        IpAddr::V6(v6) => {
            // IPv4-mapped (::ffff:a.b.c.d) → classify the embedded IPv4.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return ip_is_unsafe(&IpAddr::V4(v4));
            }
            v6.is_loopback()                              // ::1
                || v6.is_unspecified()                    // ::
                || (v6.segments()[0] & 0xfe00) == 0xfc00  // fc00::/7 unique-local
                || (v6.segments()[0] & 0xffc0) == 0xfe80 // fe80::/10 link-local
        }
    }
}

/// Check if a redirect is safe to follow (same host, no credentials).
fn is_safe_redirect(original: &Url, redirect: &Url) -> bool {
    // Must be same scheme or upgrade to https
    if redirect.scheme() != original.scheme() && redirect.scheme() != "https" {
        return false;
    }
    // Must be same host (with www. tolerance)
    let strip_www = |h: &str| h.strip_prefix("www.").unwrap_or(h).to_lowercase();
    let orig_host = original.host_str().unwrap_or("");
    let redir_host = redirect.host_str().unwrap_or("");
    if strip_www(orig_host) != strip_www(redir_host) {
        return false;
    }
    // No credentials in redirect
    if !redirect.username().is_empty() || redirect.password().is_some() {
        return false;
    }
    true
}

/// Fetch a URL, extract text content, with caching and redirect handling.
pub async fn fetch_url(
    url_str: &str,
    cache: &FetchCache,
    client: &Client,
) -> Result<FetchResult, String> {
    // Validate
    let mut parsed = validate_url(url_str)?;

    // Upgrade http → https
    if parsed.scheme() == "http" {
        let _ = parsed.set_scheme("https");
    }
    let fetch_url = parsed.to_string();

    // Check cache
    if let Some(cached) = cache.get(&fetch_url) {
        return Ok(FetchResult {
            content: cached,
            status: 200,
            url: fetch_url,
            content_type: "text/html".into(),
        });
    }

    // Fetch with redirect handling
    let mut current_url = fetch_url.clone();
    let mut redirects = 0u8;

    loop {
        let resp = client
            .get(&current_url)
            .header("User-Agent", "Crown/1.0 (compatible; bot)")
            .header("Accept", "text/html,text/markdown,text/plain,*/*")
            .timeout(FETCH_TIMEOUT)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    "Request timed out (60s)".to_string()
                } else if e.is_connect() {
                    format!("Connection failed: {}", e)
                } else {
                    format!("HTTP request failed: {}", e)
                }
            })?;

        let status = resp.status().as_u16();

        // Handle redirects manually
        if (301..=308).contains(&status) {
            if redirects >= MAX_REDIRECTS {
                return Err(format!("Too many redirects (exceeded {})", MAX_REDIRECTS));
            }
            let location = resp
                .headers()
                .get("location")
                .and_then(|v| v.to_str().ok())
                .ok_or("Redirect missing Location header")?
                .to_string();

            let redirect_url = Url::parse(&location)
                .or_else(|_| Url::parse(&current_url).and_then(|base| base.join(&location)))
                .map_err(|e| format!("Invalid redirect URL: {}", e))?;

            let original_parsed =
                Url::parse(&current_url).map_err(|e| format!("Invalid URL: {}", e))?;
            if is_safe_redirect(&original_parsed, &redirect_url) {
                current_url = redirect_url.to_string();
                redirects += 1;
                continue;
            } else {
                // Cross-host redirect — inform model
                return Ok(FetchResult {
                    content: format!(
                        "REDIRECT: {} redirects to a different host: {}\nPlease call web_fetch with the new URL if you want to follow it.",
                        url_str, redirect_url
                    ),
                    status,
                    url: url_str.to_string(),
                    content_type: "text/plain".into(),
                });
            }
        }

        if !resp.status().is_success() {
            return Err(format!(
                "HTTP {} {}",
                status,
                resp.status().canonical_reason().unwrap_or("Unknown")
            ));
        }

        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("text/html")
            .to_string();

        // Read body with size limit
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| format!("Failed to read response body: {}", e))?;

        if bytes.len() > MAX_CONTENT_LENGTH {
            return Err(format!(
                "Response too large ({} bytes, max {})",
                bytes.len(),
                MAX_CONTENT_LENGTH
            ));
        }

        let raw_text = String::from_utf8_lossy(&bytes).to_string();

        // Extract content based on content type
        let extracted = if content_type.contains("text/html") {
            extract_text_from_html(&raw_text)
        } else {
            // Plain text / markdown — use as-is
            raw_text
        };

        // Truncate if too long
        let content = if extracted.len() > MAX_OUTPUT_LENGTH {
            let mut truncated = extracted[..MAX_OUTPUT_LENGTH].to_string();
            truncated.push_str("\n\n[Content truncated — exceeded 80K character limit]");
            truncated
        } else {
            extracted
        };

        // Cache the result
        cache.put(fetch_url.clone(), content.clone(), status);

        return Ok(FetchResult {
            content,
            status,
            url: fetch_url,
            content_type,
        });
    }
}

/// Extract readable text from HTML, stripping nav/footer/script etc.
fn extract_text_from_html(html: &str) -> String {
    let document = Html::parse_document(html);

    // Try to find main content areas first
    let content_selectors = [
        "article",
        "main",
        "[role=\"main\"]",
        ".post-content",
        ".entry-content",
        ".content",
        "#content",
    ];

    for sel_str in content_selectors {
        if let Ok(selector) = Selector::parse(sel_str) {
            let elements: Vec<_> = document.select(&selector).collect();
            if !elements.is_empty() {
                let text: String = elements
                    .iter()
                    .map(|el| extract_element_text(el))
                    .collect::<Vec<_>>()
                    .join("\n\n");
                if text.len() > 100 {
                    return clean_text(&text);
                }
            }
        }
    }

    // Fallback: extract from body, excluding noise elements
    if let Ok(body_sel) = Selector::parse("body") {
        if let Some(body) = document.select(&body_sel).next() {
            let text = extract_element_text(&body);
            return clean_text(&text);
        }
    }

    // Last resort: all text
    clean_text(&document.root_element().text().collect::<String>())
}

/// Recursively extract text from an element, skipping script/style/nav/footer.
fn extract_element_text(element: &ElementRef) -> String {
    // Cap recursion depth: fetched HTML is untrusted and may be pathologically
    // deeply nested, which would overflow the stack. 256 is far deeper than
    // any real document while staying well within the stack budget.
    const MAX_DEPTH: usize = 256;
    extract_element_text_depth(element, 0, MAX_DEPTH)
}

fn extract_element_text_depth(element: &ElementRef, depth: usize, max_depth: usize) -> String {
    if depth >= max_depth {
        return String::new();
    }
    let skip_tags = [
        "script", "style", "nav", "footer", "header", "aside", "noscript", "svg", "iframe",
    ];

    let mut text = String::new();
    for node in element.children() {
        match node.value() {
            scraper::node::Node::Text(t) => {
                let trimmed = t.text.trim();
                if !trimmed.is_empty() {
                    text.push_str(trimmed);
                    text.push(' ');
                }
            }
            scraper::node::Node::Element(el) => {
                let tag = el.name();
                if skip_tags.contains(&tag) {
                    continue;
                }
                if let Some(child_ref) = ElementRef::wrap(node) {
                    let child_text = extract_element_text_depth(&child_ref, depth + 1, max_depth);
                    if !child_text.is_empty() {
                        // Add paragraph breaks for block elements
                        if matches!(
                            tag,
                            "p" | "div"
                                | "h1"
                                | "h2"
                                | "h3"
                                | "h4"
                                | "h5"
                                | "h6"
                                | "li"
                                | "br"
                                | "tr"
                        ) {
                            text.push('\n');
                        }
                        text.push_str(&child_text);
                        if matches!(
                            tag,
                            "p" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "br"
                        ) {
                            text.push('\n');
                        }
                    }
                }
            }
            _ => {}
        }
    }
    text
}

/// Clean extracted text: collapse whitespace, remove excessive blank lines.
fn clean_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut consecutive_empty = 0u32;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            consecutive_empty += 1;
            // Allow at most one blank line (two newlines)
            if consecutive_empty <= 1 {
                result.push('\n');
            }
        } else {
            consecutive_empty = 0;
            result.push_str(trimmed);
            result.push('\n');
        }
    }

    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_url_accepts_https() {
        assert!(validate_url("https://example.com/path").is_ok());
    }

    #[test]
    fn validate_url_accepts_http() {
        assert!(validate_url("http://example.com").is_ok());
    }

    #[test]
    fn validate_url_rejects_ftp() {
        assert!(validate_url("ftp://example.com").is_err());
    }

    #[test]
    fn validate_url_rejects_credentials() {
        assert!(validate_url("https://user:pass@example.com").is_err());
    }

    #[test]
    fn validate_url_rejects_localhost() {
        assert!(validate_url("https://localhost/secret").is_err());
        assert!(validate_url("https://127.0.0.1/admin").is_err());
    }

    #[test]
    fn validate_url_rejects_private_ip() {
        assert!(validate_url("https://192.168.1.1/admin").is_err());
        assert!(validate_url("https://10.0.0.1/internal").is_err());
        assert!(validate_url("https://172.16.0.1/private").is_err());
    }

    /// Security (P1-5): SSRF guard must cover the vectors a naive prefix
    /// check misses — cloud metadata, 0.0.0.0, all of 127/8, link-local,
    /// IPv6 loopback/private/link-local, IPv4-mapped IPv6, and integer-encoded
    /// IPs.
    #[test]
    fn validate_url_rejects_extended_ssrf_vectors() {
        // Cloud metadata endpoint (AWS/GCP/Azure) — the classic SSRF target.
        assert!(validate_url("https://169.254.169.254/latest/meta-data/").is_err());
        // Link-local range generally.
        assert!(validate_url("https://169.254.1.1/").is_err());
        // Unspecified address.
        assert!(validate_url("https://0.0.0.0/").is_err());
        // All of 127/8, not just 127.0.0.1.
        assert!(validate_url("https://127.0.0.2/").is_err());
        assert!(validate_url("https://127.1.2.3/").is_err());
        // Other private ranges.
        assert!(validate_url("https://172.31.255.255/").is_err());
        // IPv6 loopback / private / link-local (bracketed in URLs).
        assert!(validate_url("https://[::1]/").is_err());
        assert!(validate_url("https://[fc00::1]/").is_err());
        assert!(validate_url("https://[fe80::1]/").is_err());
        // IPv4-mapped IPv6 pointing at loopback.
        assert!(validate_url("https://[::ffff:127.0.0.1]/").is_err());
        // Integer / hex / octal encoded loopback (2130706433 == 127.0.0.1).
        assert!(validate_url("https://2130706433/").is_err());
        assert!(validate_url("https://0x7f000001/").is_err());
    }

    /// Public hosts and IPs must still be allowed (no false positives).
    #[test]
    fn validate_url_allows_public() {
        assert!(validate_url("https://example.com/path").is_ok());
        assert!(validate_url("https://8.8.8.8/").is_ok());
        assert!(validate_url("https://1.1.1.1/").is_ok());
        assert!(validate_url("https://[2606:4700:4700::1111]/").is_ok());
    }

    #[test]
    fn validate_url_rejects_too_long() {
        let long_url = format!("https://example.com/{}", "a".repeat(2000));
        assert!(validate_url(&long_url).is_err());
    }

    #[test]
    fn html_extraction_basic() {
        let html =
            "<html><body><h1>Title</h1><p>Hello world</p><script>evil()</script></body></html>";
        let text = extract_text_from_html(html);
        assert!(text.contains("Title"));
        assert!(text.contains("Hello world"));
        assert!(!text.contains("evil"));
    }

    #[test]
    fn html_extraction_article_priority() {
        let html = r#"<html><body><nav>Menu</nav><article><p>Important content here that is long enough to pass the threshold check in our extraction logic for article elements</p></article><footer>Footer</footer></body></html>"#;
        let text = extract_text_from_html(html);
        assert!(text.contains("Important content"));
    }

    #[test]
    fn safe_redirect_same_host() {
        let orig = Url::parse("https://example.com/a").unwrap();
        let redir = Url::parse("https://example.com/b").unwrap();
        assert!(is_safe_redirect(&orig, &redir));
    }

    #[test]
    fn safe_redirect_www_added() {
        let orig = Url::parse("https://example.com/a").unwrap();
        let redir = Url::parse("https://www.example.com/b").unwrap();
        assert!(is_safe_redirect(&orig, &redir));
    }

    #[test]
    fn unsafe_redirect_different_host() {
        let orig = Url::parse("https://example.com/a").unwrap();
        let redir = Url::parse("https://evil.com/b").unwrap();
        assert!(!is_safe_redirect(&orig, &redir));
    }

    #[test]
    fn cache_stores_and_retrieves() {
        let cache = FetchCache::new();
        cache.put("https://example.com".into(), "hello".into(), 200);
        assert_eq!(cache.get("https://example.com"), Some("hello".into()));
    }

    #[test]
    fn cache_returns_none_for_missing() {
        let cache = FetchCache::new();
        assert_eq!(cache.get("https://not-cached.com"), None);
    }

    #[test]
    fn clean_text_collapses_blank_lines() {
        let input = "Hello\n\n\n\n\n\nWorld";
        let result = clean_text(input);
        assert!(!result.contains("\n\n\n\n"));
        assert!(result.contains("Hello"));
        assert!(result.contains("World"));
    }
}
