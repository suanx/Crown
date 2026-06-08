use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use reqwest::StatusCode;
use tracing::warn;

/// Configuration for retry behavior with exponential backoff.
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of attempts (including the initial request).
    pub max_attempts: u32,
    /// Initial backoff duration in milliseconds.
    pub initial_backoff_ms: u64,
    /// Maximum backoff duration in milliseconds.
    pub max_backoff_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_attempts: 4,
            initial_backoff_ms: 500,
            max_backoff_ms: 10_000,
        }
    }
}

/// HTTP status codes that are considered retryable.
const RETRYABLE_STATUS_CODES: &[u16] = &[408, 429, 500, 502, 503, 504];

/// Check if a status code is retryable.
fn is_retryable(status: StatusCode) -> bool {
    RETRYABLE_STATUS_CODES.contains(&status.as_u16())
}

/// Apply jitter (±25%) to a backoff duration.
/// Uses system time nanos as a simple pseudo-random source.
fn apply_jitter(backoff_ms: u64) -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    // random_factor in [0.0, 1.0)
    let random_factor = (nanos % 1000) as f64 / 1000.0;
    // jitter_range is ±25%, so factor is in [0.75, 1.25)
    let jitter_factor = 0.75 + (random_factor * 0.5);
    (backoff_ms as f64 * jitter_factor) as u64
}

/// Extract the Retry-After header as a delay from `now`.
///
/// Per RFC 7231 §7.1.3, Retry-After is either a non-negative integer count of
/// seconds OR an HTTP-date. We support both: a bare integer, or an IMF-fixdate
/// (`Sun, 06 Nov 1994 08:49:37 GMT`) whose delta from `now` is returned. A
/// date in the past yields `Some(0)` (retry immediately). Unparseable values
/// yield `None` (caller falls back to exponential backoff).
fn get_retry_after(response: &reqwest::Response) -> Option<u64> {
    let raw = response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())?;
    parse_retry_after(raw, SystemTime::now()).map(|d| d.as_secs())
}

/// Pure, testable Retry-After parser. See [`get_retry_after`].
fn parse_retry_after(value: &str, now: SystemTime) -> Option<Duration> {
    let v = value.trim();
    // Integer seconds form.
    if let Ok(secs) = v.parse::<u64>() {
        return Some(Duration::from_secs(secs));
    }
    // HTTP-date (IMF-fixdate) form.
    let target = parse_imf_fixdate(v)?;
    let now_secs = now.duration_since(UNIX_EPOCH).ok()?.as_secs();
    Some(Duration::from_secs(target.saturating_sub(now_secs)))
}

/// Parse an IMF-fixdate (`Sun, 06 Nov 1994 08:49:37 GMT`) into a Unix
/// timestamp (seconds). Returns `None` for any other format. Self-contained
/// (no chrono dependency) — handles the single canonical HTTP-date form that
/// servers are required to send.
fn parse_imf_fixdate(s: &str) -> Option<u64> {
    // "Sun, 06 Nov 1994 08:49:37 GMT"
    let s = s.strip_suffix(" GMT")?;
    let comma = s.find(", ")?;
    let rest = &s[comma + 2..]; // "06 Nov 1994 08:49:37"
    let mut parts = rest.split(' ');
    let day: u64 = parts.next()?.parse().ok()?;
    let month = match parts.next()? {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    };
    let year: u64 = parts.next()?.parse().ok()?;
    let time = parts.next()?; // "08:49:37"
    let mut tparts = time.split(':');
    let hour: u64 = tparts.next()?.parse().ok()?;
    let min: u64 = tparts.next()?.parse().ok()?;
    let sec: u64 = tparts.next()?.parse().ok()?;
    Some(ymd_hms_to_unix(year, month, day, hour, min, sec))
}

/// Convert a UTC civil date-time to a Unix timestamp (seconds). Uses the
/// standard days-from-civil algorithm (valid for the Gregorian calendar).
fn ymd_hms_to_unix(year: u64, month: u64, day: u64, hour: u64, min: u64, sec: u64) -> u64 {
    // days_from_civil (Howard Hinnant's algorithm), specialized for years
    // we'll realistically see (>= 1970), all arithmetic non-negative.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468; // days since 1970-01-01
    days * 86_400 + hour * 3_600 + min * 60 + sec
}

/// Execute an HTTP request with retry logic and exponential backoff.
///
/// The `request_builder_fn` closure is called for each attempt because
/// `RequestBuilder` is consumed on `.send()`.
pub async fn fetch_with_retry(
    _client: &reqwest::Client,
    request_builder_fn: impl Fn() -> reqwest::RequestBuilder,
    config: &RetryConfig,
) -> Result<reqwest::Response> {
    let mut last_error: Option<anyhow::Error> = None;

    for attempt in 1..=config.max_attempts {
        let request = request_builder_fn();

        match request.send().await {
            Ok(response) => {
                let status = response.status();
                if status.is_success() || status.is_redirection() {
                    return Ok(response);
                }

                if !is_retryable(status) || attempt == config.max_attempts {
                    let body = response
                        .text()
                        .await
                        .unwrap_or_else(|_| "<failed to read body>".to_string());
                    return Err(anyhow!("HTTP {} — {}", status.as_u16(), body));
                }

                // Calculate backoff
                let retry_after_ms = get_retry_after(&response).map(|s| s * 1000);
                let exponential_ms = config.initial_backoff_ms * 2u64.pow(attempt - 1);
                let capped_ms = exponential_ms.min(config.max_backoff_ms);
                let backoff_ms = match retry_after_ms {
                    Some(ra) => ra.max(capped_ms),
                    None => apply_jitter(capped_ms),
                };

                warn!(
                    attempt,
                    max_attempts = config.max_attempts,
                    status = status.as_u16(),
                    backoff_ms,
                    "Retryable HTTP error, backing off"
                );

                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                last_error = Some(anyhow!("HTTP {}", status.as_u16()));
            }
            Err(e) => {
                if attempt == config.max_attempts {
                    return Err(anyhow!(
                        "Request failed after {} attempts: {}",
                        config.max_attempts,
                        e
                    ));
                }

                let exponential_ms = config.initial_backoff_ms * 2u64.pow(attempt - 1);
                let capped_ms = exponential_ms.min(config.max_backoff_ms);
                let backoff_ms = apply_jitter(capped_ms);

                warn!(
                    attempt,
                    max_attempts = config.max_attempts,
                    error = %e,
                    backoff_ms,
                    "Request error, retrying"
                );

                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                last_error = Some(e.into());
            }
        }
    }

    Err(last_error
        .unwrap_or_else(|| anyhow!("Request failed after {} attempts", config.max_attempts)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_after_integer_seconds() {
        let now = UNIX_EPOCH + Duration::from_secs(1_000_000);
        assert_eq!(
            parse_retry_after("120", now),
            Some(Duration::from_secs(120))
        );
        assert_eq!(parse_retry_after("  0 ", now), Some(Duration::from_secs(0)));
    }

    #[test]
    fn imf_fixdate_known_epoch() {
        // Unix epoch itself: "Thu, 01 Jan 1970 00:00:00 GMT" → 0.
        assert_eq!(parse_imf_fixdate("Thu, 01 Jan 1970 00:00:00 GMT"), Some(0));
        // A well-known timestamp: 2001-09-09 01:46:40 UTC == 1_000_000_000.
        assert_eq!(
            parse_imf_fixdate("Sun, 09 Sep 2001 01:46:40 GMT"),
            Some(1_000_000_000)
        );
    }

    #[test]
    fn retry_after_http_date_future_delta() {
        // now = 1_000_000_000; target 100s later.
        let now = UNIX_EPOCH + Duration::from_secs(1_000_000_000);
        let got = parse_retry_after("Sun, 09 Sep 2001 01:48:20 GMT", now);
        assert_eq!(got, Some(Duration::from_secs(100)));
    }

    #[test]
    fn retry_after_http_date_in_past_is_zero() {
        let now = UNIX_EPOCH + Duration::from_secs(2_000_000_000);
        // target is in the past relative to now → saturating to 0.
        let got = parse_retry_after("Sun, 09 Sep 2001 01:46:40 GMT", now);
        assert_eq!(got, Some(Duration::from_secs(0)));
    }

    #[test]
    fn retry_after_garbage_is_none() {
        let now = SystemTime::now();
        assert_eq!(parse_retry_after("not-a-date", now), None);
        assert_eq!(
            parse_retry_after("Mon, 32 Xyz 1994 99:99:99 GMT", now),
            None
        );
    }

    #[test]
    fn jitter_stays_in_range() {
        for _ in 0..50 {
            let j = apply_jitter(1000);
            assert!((750..=1250).contains(&j), "jitter {j} out of ±25% range");
        }
    }
}
