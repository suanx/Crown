//! Web search providers.

use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::warn;

use super::config::WebConfig;

/// A single search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

/// Perform a web search using the configured provider.
/// Returns up to `max_results` results.
pub async fn web_search(
    query: &str,
    max_results: usize,
    config: &WebConfig,
    client: &Client,
) -> Result<Vec<SearchResult>, String> {
    match config.search_provider.as_str() {
        "bocha" => {
            let key = config
                .bocha_api_key
                .as_deref()
                .ok_or_else(|| "Bocha search requires bocha_api_key in config".to_string())?;
            bocha_search(query, max_results, key, client).await
        }
        "brave" => {
            let key = config
                .brave_api_key
                .as_deref()
                .ok_or_else(|| "Brave search requires brave_api_key in config".to_string())?;
            brave_search(query, max_results, key, client).await
        }
        "tavily" => {
            let key = config
                .tavily_api_key
                .as_deref()
                .ok_or_else(|| "Tavily search requires tavily_api_key in config".to_string())?;
            tavily_search(query, max_results, key, client).await
        }
        "exa" => {
            let key = config
                .exa_api_key
                .as_deref()
                .ok_or_else(|| "Exa search requires exa_api_key in config".to_string())?;
            exa_search(query, max_results, key, client).await
        }
        "serper" => {
            let key = config
                .serper_api_key
                .as_deref()
                .ok_or_else(|| "Serper search requires serper_api_key in config".to_string())?;
            serper_search(query, max_results, key, client).await
        }
        "serpapi" => {
            let key = config
                .serpapi_api_key
                .as_deref()
                .ok_or_else(|| "SerpAPI search requires serpapi_api_key in config".to_string())?;
            serpapi_search(query, max_results, key, client).await
        }
        "duckduckgo" => duckduckgo_search(query, max_results, client).await,
        "jina" => {
            let jina_key = config.jina_api_key.as_deref().filter(|k| !k.is_empty());
            if jina_key.is_some() {
                jina_search(query, max_results, jina_key, client).await
            } else {
                duckduckgo_search(query, max_results, client).await
            }
        }
        _ => {
            // Default: try Jina if key is available, else DuckDuckGo (truly zero-config)
            let jina_key = config.jina_api_key.as_deref().filter(|k| !k.is_empty());
            if jina_key.is_some() {
                jina_search(query, max_results, jina_key, client).await
            } else {
                duckduckgo_search(query, max_results, client).await
            }
        }
    }
}

/// Tavily Search: POST https://api.tavily.com/search
async fn tavily_search(
    query: &str,
    max_results: usize,
    api_key: &str,
    client: &Client,
) -> Result<Vec<SearchResult>, String> {
    let body = serde_json::json!({
        "query": query,
        "max_results": max_results.min(10),
        "search_depth": "basic",
        "include_answer": false,
        "include_raw_content": false,
    });

    let resp = client
        .post("https://api.tavily.com/search")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(30))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Tavily search failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "Tavily search returned {}: {}",
            status,
            text.chars().take(200).collect::<String>()
        ));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Tavily: invalid JSON: {}", e))?;

    let mut results = Vec::new();
    if let Some(items) = data.get("results").and_then(|v| v.as_array()) {
        for item in items.iter().take(max_results) {
            let title = item
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let url = item
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let snippet = item
                .get("content")
                .or_else(|| item.get("snippet"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .chars()
                .take(300)
                .collect::<String>();
            if !url.is_empty() {
                results.push(SearchResult {
                    title,
                    url,
                    snippet,
                });
            }
        }
    }

    Ok(results)
}

/// Jina Search: GET https://s.jina.ai/{query}
/// Returns markdown-formatted search results. No key required (20 RPM limit).
async fn jina_search(
    query: &str,
    max_results: usize,
    api_key: Option<&str>,
    client: &Client,
) -> Result<Vec<SearchResult>, String> {
    let url = format!("https://s.jina.ai/{}", urlencoding::encode(query));

    // If we have an API key, request JSON (structured results).
    // Without a key, Jina returns 401 for JSON mode — fall back to markdown parsing.
    let has_key = api_key.is_some_and(|k| !k.is_empty());

    let mut req = client
        .get(&url)
        .header("X-Retain-Images", "none")
        .timeout(std::time::Duration::from_secs(30));

    if has_key {
        req = req.header("Accept", "application/json");
        req = req.header(
            "Authorization",
            format!("Bearer {}", api_key.unwrap_or_default()),
        );
    } else {
        // Without key: request plain markdown (no Accept: application/json)
        req = req.header("Accept", "text/plain");
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("Jina search failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!(
            "Jina search returned {}: {}",
            status,
            body.chars().take(200).collect::<String>()
        ));
    }

    if has_key {
        // JSON mode: structured response with `data` array
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Jina search: invalid JSON: {}", e))?;

        let mut results = Vec::new();
        if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
            for item in data.iter().take(max_results) {
                let title = item
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let url = item
                    .get("url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let snippet = item
                    .get("description")
                    .or_else(|| item.get("content"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .chars()
                    .take(300)
                    .collect::<String>();
                if !url.is_empty() {
                    results.push(SearchResult {
                        title,
                        url,
                        snippet,
                    });
                }
            }
        }

        if results.is_empty() {
            warn!(
                "Jina search (JSON mode) returned no structured results, query: {}",
                query
            );
        }
        Ok(results)
    } else {
        // Markdown mode: parse the plain text response into results
        let text = resp
            .text()
            .await
            .map_err(|e| format!("Jina search: failed to read response: {}", e))?;

        let results = parse_jina_markdown(&text, max_results);

        if results.is_empty() {
            warn!(
                "Jina search (markdown mode) returned no parseable results, query: {}",
                query
            );
        }
        Ok(results)
    }
}

/// Parse Jina's markdown search results into structured data.
///
/// Jina markdown format (typical):
/// ```text
/// [Title](URL)
/// Description text here...
///
/// [Title 2](URL2)
/// More description...
/// ```
fn parse_jina_markdown(text: &str, max_results: usize) -> Vec<SearchResult> {
    let mut results = Vec::new();
    let mut current_title = String::new();
    let mut current_url = String::new();
    let mut current_snippet = String::new();

    for line in text.lines() {
        let trimmed = line.trim();

        // Match markdown link: [Title](URL)
        if trimmed.starts_with('[') {
            // Flush previous result
            if !current_url.is_empty() {
                results.push(SearchResult {
                    title: current_title.clone(),
                    url: current_url.clone(),
                    snippet: current_snippet.trim().to_string(),
                });
                if results.len() >= max_results {
                    return results;
                }
            }
            current_title.clear();
            current_url.clear();
            current_snippet.clear();

            // Parse [title](url)
            if let Some(close_bracket) = trimmed.find("](") {
                current_title = trimmed[1..close_bracket].to_string();
                let rest = &trimmed[close_bracket + 2..];
                if let Some(close_paren) = rest.find(')') {
                    current_url = rest[..close_paren].to_string();
                }
            }
        } else if !current_url.is_empty() && !trimmed.is_empty() {
            // Accumulate snippet text
            if current_snippet.len() < 300 {
                if !current_snippet.is_empty() {
                    current_snippet.push(' ');
                }
                current_snippet.push_str(trimmed);
            }
        }
    }

    // Flush last result
    if !current_url.is_empty() && results.len() < max_results {
        results.push(SearchResult {
            title: current_title,
            url: current_url,
            snippet: current_snippet.trim().to_string(),
        });
    }

    results
}

/// Bocha Search: POST https://api.bochaai.com/v1/web/search
async fn bocha_search(
    query: &str,
    max_results: usize,
    api_key: &str,
    client: &Client,
) -> Result<Vec<SearchResult>, String> {
    let body = serde_json::json!({
        "query": query,
        "count": max_results.min(10),
        "summary": true,
    });

    let resp = client
        .post("https://api.bochaai.com/v1/web-search")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(30))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Bocha search failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "Bocha search returned {}: {}",
            status,
            text.chars().take(200).collect::<String>()
        ));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Bocha: invalid JSON: {}", e))?;

    let mut results = Vec::new();
    // Bocha returns { "data": { "webPages": { "value": [...] } } }
    if let Some(pages) = data
        .pointer("/data/webPages/value")
        .and_then(|v| v.as_array())
    {
        for item in pages.iter().take(max_results) {
            let title = item
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let url = item
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let snippet = item
                .get("summary")
                .or_else(|| item.get("snippet"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if !url.is_empty() {
                results.push(SearchResult {
                    title,
                    url,
                    snippet,
                });
            }
        }
    }

    Ok(results)
}

/// Brave Search: GET https://api.search.brave.com/res/v1/web/search
async fn brave_search(
    query: &str,
    max_results: usize,
    api_key: &str,
    client: &Client,
) -> Result<Vec<SearchResult>, String> {
    let url = format!(
        "https://api.search.brave.com/res/v1/web/search?q={}&count={}",
        urlencoding::encode(query),
        max_results.min(20)
    );

    let resp = client
        .get(&url)
        .header("X-Subscription-Token", api_key)
        .header("Accept", "application/json")
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("Brave search failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "Brave search returned {}: {}",
            status,
            text.chars().take(200).collect::<String>()
        ));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Brave: invalid JSON: {}", e))?;

    let mut results = Vec::new();
    if let Some(web_results) = data.pointer("/web/results").and_then(|v| v.as_array()) {
        for item in web_results.iter().take(max_results) {
            let title = item
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let url = item
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let snippet = item
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if !url.is_empty() {
                results.push(SearchResult {
                    title,
                    url,
                    snippet,
                });
            }
        }
    }

    Ok(results)
}

/// DuckDuckGo HTML search — truly zero-config, no API key needed.
///
/// Uses DuckDuckGo's HTML-only endpoint which doesn't require authentication.
/// Parses results from the HTML response. Works in mainland China without VPN.
async fn duckduckgo_search(
    query: &str,
    max_results: usize,
    client: &Client,
) -> Result<Vec<SearchResult>, String> {
    let resp = client
        .get("https://html.duckduckgo.com/html/")
        .query(&[("q", query)])
        .header("User-Agent", "Crown/1.0 (compatible; bot)")
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("DuckDuckGo search failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        return Err(format!("DuckDuckGo returned HTTP {}", status));
    }

    let html = resp
        .text()
        .await
        .map_err(|e| format!("DuckDuckGo: failed to read body: {}", e))?;

    // Parse results from DuckDuckGo HTML
    let results = parse_ddg_html(&html, max_results);

    if results.is_empty() {
        warn!(
            "DuckDuckGo returned no parseable results for query: {}",
            query
        );
    }

    Ok(results)
}

/// Parse DuckDuckGo HTML search results.
///
/// DDG HTML format contains result blocks with class "result":
/// ```html
/// <div class="result">
///   <a class="result__a" href="URL">Title</a>
///   <a class="result__snippet">Snippet text...</a>
/// </div>
/// ```
fn parse_ddg_html(html: &str, max_results: usize) -> Vec<SearchResult> {
    use scraper::{Html, Selector};

    let document = Html::parse_document(html);
    let mut results = Vec::new();

    // DuckDuckGo uses class "result__a" for the title link
    let link_sel = Selector::parse("a.result__a").unwrap_or_else(|_| {
        // Fallback selector
        Selector::parse("a[href]").unwrap()
    });
    let snippet_sel = Selector::parse("a.result__snippet, .result__snippet")
        .unwrap_or_else(|_| Selector::parse(".snippet").unwrap());

    let result_sel =
        Selector::parse(".result, .web-result").unwrap_or_else(|_| Selector::parse("div").unwrap());

    for result_el in document.select(&result_sel).take(max_results * 2) {
        // Find the title link
        let Some(link) = result_el.select(&link_sel).next() else {
            continue;
        };
        let title = link.text().collect::<String>().trim().to_string();
        let href = link.value().attr("href").unwrap_or("").to_string();

        if title.is_empty() || href.is_empty() {
            continue;
        }

        // DuckDuckGo sometimes wraps URLs in a redirect — extract the actual URL
        let url = if href.contains("uddg=") {
            // Extract from redirect: //duckduckgo.com/l/?uddg=ENCODED_URL&...
            href.split("uddg=")
                .nth(1)
                .and_then(|s| s.split('&').next())
                .and_then(|encoded| urlencoding::decode(encoded).ok())
                .map(|s| s.to_string())
                .unwrap_or(href)
        } else if href.starts_with("//") {
            format!("https:{}", href)
        } else {
            href
        };

        // Skip DuckDuckGo internal links
        if url.contains("duckduckgo.com") {
            continue;
        }

        // Find snippet
        let snippet = result_el
            .select(&snippet_sel)
            .next()
            .map(|el| el.text().collect::<String>().trim().to_string())
            .unwrap_or_default();

        results.push(SearchResult {
            title,
            url,
            snippet,
        });

        if results.len() >= max_results {
            break;
        }
    }

    results
}

/// Exa Search: POST https://api.exa.ai/search
///
/// Exa (formerly Metaphor) provides neural search with content extraction.
/// Docs: https://docs.exa.ai/reference/search
async fn exa_search(
    query: &str,
    max_results: usize,
    api_key: &str,
    client: &Client,
) -> Result<Vec<SearchResult>, String> {
    let body = serde_json::json!({
        "query": query,
        "numResults": max_results.min(10),
        "type": "keyword",
        "contents": {
            "text": true
        }
    });

    let resp = client
        .post("https://api.exa.ai/search")
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("x-api-key", api_key)
        .timeout(std::time::Duration::from_secs(30))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Exa search failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "Exa search returned {}: {}",
            status,
            text.chars().take(200).collect::<String>()
        ));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Exa: invalid JSON: {}", e))?;

    let mut results = Vec::new();
    if let Some(items) = data.get("results").and_then(|v| v.as_array()) {
        for item in items.iter().take(max_results) {
            let title = item
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let url = item
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let snippet = item
                .pointer("/contents/text")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .chars()
                .take(300)
                .collect::<String>();
            if !url.is_empty() {
                results.push(SearchResult {
                    title,
                    url,
                    snippet,
                });
            }
        }
    }

    Ok(results)
}

/// Serper Search: POST https://google.serper.dev/search
///
/// Serper provides a Google Search API at competitive pricing.
/// Docs: https://serper.dev
async fn serper_search(
    query: &str,
    max_results: usize,
    api_key: &str,
    client: &Client,
) -> Result<Vec<SearchResult>, String> {
    let body = serde_json::json!({
        "q": query,
        "num": max_results.min(10),
    });

    let resp = client
        .post("https://google.serper.dev/search")
        .header("X-API-KEY", api_key)
        .header("Content-Type", "application/json")
        .timeout(std::time::Duration::from_secs(30))
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("Serper search failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "Serper search returned {}: {}",
            status,
            text.chars().take(200).collect::<String>()
        ));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("Serper: invalid JSON: {}", e))?;

    let mut results = Vec::new();
    if let Some(items) = data.get("organic").and_then(|v| v.as_array()) {
        for item in items.iter().take(max_results) {
            let title = item
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let url = item
                .get("link")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let snippet = item
                .get("snippet")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if !url.is_empty() {
                results.push(SearchResult {
                    title,
                    url,
                    snippet,
                });
            }
        }
    }

    Ok(results)
}

/// SerpAPI Search: GET https://serpapi.com/search
///
/// SerpAPI provides Google Search results via JSON API.
/// Docs: https://serpapi.com/search-api
async fn serpapi_search(
    query: &str,
    max_results: usize,
    api_key: &str,
    client: &Client,
) -> Result<Vec<SearchResult>, String> {
    let resp = client
        .get("https://serpapi.com/search")
        .query(&[
            ("q", query),
            ("api_key", api_key),
            ("num", &max_results.min(10).to_string()),
            ("engine", "google"),
        ])
        .header("Accept", "application/json")
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("SerpAPI search failed: {}", e))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(format!(
            "SerpAPI search returned {}: {}",
            status,
            text.chars().take(200).collect::<String>()
        ));
    }

    let data: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("SerpAPI: invalid JSON: {}", e))?;

    let mut results = Vec::new();
    if let Some(items) = data.get("organic_results").and_then(|v| v.as_array()) {
        for item in items.iter().take(max_results) {
            let title = item
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let url = item
                .get("link")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let snippet = item
                .get("snippet")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if !url.is_empty() {
                results.push(SearchResult {
                    title,
                    url,
                    snippet,
                });
            }
        }
    }

    // Also try the "answer_box" for direct answers if no organic results
    if results.is_empty() {
        if let Some(answer) = data.get("answer_box") {
            let title = answer
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let snippet = answer
                .get("answer")
                .or_else(|| answer.get("snippet"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let url = answer
                .get("link")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if !url.is_empty() || !snippet.is_empty() {
                results.push(SearchResult {
                    title,
                    url,
                    snippet,
                });
            }
        }
    }

    Ok(results)
}
