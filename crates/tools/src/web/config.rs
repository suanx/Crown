//! Web tools configuration.

use serde::{Deserialize, Serialize};

/// Configuration for web search/fetch tools.
/// Read from config.toml `[web]` section.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WebConfig {
    /// Search provider: "jina" (default) | "duckduckgo" | "bocha" | "brave" | "tavily" | "exa" | "serper" | "serpapi"
    #[serde(default = "default_search_provider")]
    pub search_provider: String,
    /// Jina API key (optional — works without, but rate-limited to 20 RPM)
    pub jina_api_key: Option<String>,
    /// Bocha API key (optional — needed if provider = "bocha")
    pub bocha_api_key: Option<String>,
    /// Brave API key (optional — needed if provider = "brave")
    pub brave_api_key: Option<String>,
    /// Tavily API key (optional — needed if provider = "tavily")
    pub tavily_api_key: Option<String>,
    /// Exa API key (needed if provider = "exa")
    pub exa_api_key: Option<String>,
    /// Serper API key (needed if provider = "serper")
    pub serper_api_key: Option<String>,
    /// SerpAPI API key (needed if provider = "serpapi")
    pub serpapi_api_key: Option<String>,
}

fn default_search_provider() -> String {
    // "jina" routes through the zero-config branch in search.rs (Jina if a key
    // is present, else DuckDuckGo). Defaulting to "bocha" would hard-error
    // without a bocha_api_key, breaking out-of-the-box web_search.
    "jina".to_string()
}
