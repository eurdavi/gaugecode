//! OpenCode Go adapter — `Fidelity::Official`.
//!
//! Reads the `opencode-go` key OpenCode stores on sign-in and asks OpenCode's
//! own usage endpoint.
//!
//! Two upstream quirks shape this adapter. A valid key with **no** Go plan
//! answers `401`, exactly like a bad key, so the two cannot be told apart from
//! the response. And `403` means a valid key that is not entitled to Go, which
//! is metering nothing rather than an error — showing it in red would be wrong.
//!
//! Zen pay-as-you-go credit has no endpoint at all, so this covers the Go
//! windows only.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::model::{
    Fidelity, LimitWindow, ProviderAccount, ProviderError, ProviderId, ProviderSnapshot,
    SignInHint,
};
use crate::providers::{retry_after_header, UsageProvider};

const USAGE_URL: &str = "https://opencode.ai/zen/go/v1/usage";
const TIMEOUT: Duration = Duration::from_secs(15);
const MANAGE_URL: &str = "https://opencode.ai";
/// The only entry that authenticates this endpoint. Any other key in the file
/// belongs to a different vendor, and claiming one would report the wrong
/// account under OpenCode's name.
const GO_KEY: &str = "opencode-go";

// ---------------------------------------------------------------------------
// Credential
// ---------------------------------------------------------------------------

fn auth_path() -> Result<PathBuf, ProviderError> {
    if let Some(dir) = std::env::var_os("XDG_DATA_HOME") {
        return Ok(PathBuf::from(dir).join("opencode").join("auth.json"));
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
    let Some(home) = home else {
        return Err(ProviderError::Io("neither USERPROFILE nor HOME is set".into()));
    };
    Ok(PathBuf::from(home).join(".local").join("share").join("opencode").join("auth.json"))
}

/// The entry has shipped both as a bare string and as an object wrapping the
/// key under one of several names, so both shapes are accepted.
fn parse_credential(raw: &str) -> Result<String, ProviderError> {
    const KEYS: [&str; 5] = ["key", "apiKey", "api_key", "token", "accessToken"];

    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|_| ProviderError::Io("auth.json is not the expected shape".into()))?;
    let Some(entry) = value.get(GO_KEY) else {
        return Err(ProviderError::NeedsAuth);
    };

    let token = match entry {
        serde_json::Value::String(token) => Some(token.as_str()),
        _ => KEYS.iter().find_map(|key| entry.get(key).and_then(serde_json::Value::as_str)),
    };
    // An empty key is worse than a missing one: it sends a request that cannot
    // succeed.
    match token.map(str::trim) {
        Some(token) if !token.is_empty() => Ok(token.to_string()),
        _ => Err(ProviderError::NeedsAuth),
    }
}

fn read_credential() -> Result<String, ProviderError> {
    let raw = match std::fs::read_to_string(auth_path()?) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ProviderError::NeedsAuth)
        }
        Err(error) => return Err(ProviderError::Io(error.to_string())),
    };
    parse_credential(&raw)
}

// ---------------------------------------------------------------------------
// Response
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct UsageResponse {
    #[serde(default)]
    usage: Option<UsageWindows>,
}

#[derive(Debug, Deserialize)]
struct UsageWindows {
    #[serde(default)]
    rolling: Option<Window>,
    #[serde(default)]
    weekly: Option<Window>,
    #[serde(default)]
    monthly: Option<Window>,
}

#[derive(Debug, Deserialize)]
struct Window {
    /// 0–100, already *used* rather than remaining, so the ring needs no
    /// inversion. Absent means the window is absent, not zero.
    #[serde(default)]
    percent: Option<f64>,
    /// RFC 3339 with fractional seconds, e.g. `2026-09-06T12:31:06.611Z`.
    #[serde(default, rename = "resetsAt")]
    resets_at: Option<DateTime<Utc>>,
}

fn fraction(percent: f64) -> f64 {
    (percent / 100.0).clamp(0.0, 1.0)
}

/// `rolling` is the headline window — OpenCode's equivalent of Claude's session.
fn parse_usage(
    body: &str,
    fetched_at: DateTime<Utc>,
) -> Result<ProviderSnapshot, ProviderError> {
    let response: UsageResponse =
        serde_json::from_str(body).map_err(|_| ProviderError::BadResponse { status: 200 })?;
    let Some(usage) = response.usage else {
        return Err(ProviderError::BadResponse { status: 200 });
    };

    let mut windows = Vec::new();
    let mut push = |id: &str, label: &str, window: Option<Window>| {
        let Some(window) = window else { return };
        let Some(percent) = window.percent else { return };
        windows.push(LimitWindow {
            id: id.to_string(),
            label: label.to_string(),
            used_fraction: fraction(percent),
            resets_at: window.resets_at,
        });
    };
    push("rolling", "5h limit", usage.rolling);
    push("weekly", "Weekly limit", usage.weekly);
    push("monthly", "Monthly limit", usage.monthly);

    if windows.is_empty() {
        return Err(ProviderError::NothingMetered(
            "usage response carried no window".into(),
        ));
    }

    // Already in headline order, so no sort: `rolling` first by construction.
    Ok(ProviderSnapshot {
        provider: ProviderId::OpenCode,
        fidelity: Fidelity::Official,
        windows,
        fetched_at,
        // The key carries no address, and the only plan this endpoint serves
        // is Go.
        account: Some(ProviderAccount { email: None, plan: Some("Go".into()) }),
    })
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

pub struct OpenCodeProvider {
    client: reqwest::Client,
}

impl OpenCodeProvider {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .user_agent(concat!("GaugeCode/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client }
    }
}

impl Default for OpenCodeProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl UsageProvider for OpenCodeProvider {
    fn id(&self) -> ProviderId {
        ProviderId::OpenCode
    }

    fn fidelity(&self) -> Fidelity {
        Fidelity::Official
    }

    async fn fetch_snapshot(&self) -> Result<ProviderSnapshot, ProviderError> {
        let token = read_credential()?;

        let started = Instant::now();
        let response = self
            .client
            .get(USAGE_URL)
            .bearer_auth(&token)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|error| ProviderError::Io(error.without_url().to_string()))?;

        let status = response.status();
        tracing::debug!(
            provider = "opencode",
            status = status.as_u16(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "usage request finished"
        );

        match status.as_u16() {
            200..=299 => {
                let body = response
                    .text()
                    .await
                    .map_err(|error| ProviderError::Io(error.without_url().to_string()))?;
                parse_usage(&body, Utc::now())
            }
            // Upstream serves a missing Go plan as 401 through the same branch
            // as a bad key, so both read as "nothing readable here".
            401 => Err(ProviderError::NeedsAuth),
            // A valid key with no Go entitlement: readable, but metering
            // nothing. Not an error, and it must not be shown as one.
            403 => Err(ProviderError::NothingMetered(
                "no OpenCode Go subscription on this key".into(),
            )),
            429 => Err(ProviderError::RateLimited {
                retry_after: retry_after_header(response.headers()),
            }),
            other => Err(ProviderError::BadResponse { status: other }),
        }
    }

    fn account(&self) -> Option<ProviderAccount> {
        read_credential().ok()?;
        Some(ProviderAccount { email: None, plan: Some("Go".into()) })
    }

    fn forget_cached_credential(&self) {
        // Nothing is cached: the file is re-read on every fetch.
    }

    fn sign_in_hint(&self) -> SignInHint {
        SignInHint::Text {
            text: "Connect Go inside OpenCode (`opencode auth login`); GaugeCode reads the key it stores."
                .into(),
        }
    }

    fn manage_url(&self) -> &'static str {
        MANAGE_URL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/opencode_usage_ok.json");

    fn at(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text).unwrap().with_timezone(&Utc)
    }

    fn parse(body: &str) -> Result<ProviderSnapshot, ProviderError> {
        parse_usage(body, Utc::now())
    }

    #[test]
    fn parses_all_three_windows_with_rolling_as_the_headline() {
        let snapshot = parse(OK).expect("fixture should parse");
        assert_eq!(snapshot.provider, ProviderId::OpenCode);
        let ids: Vec<&str> = snapshot.windows.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, ["rolling", "weekly", "monthly"]);

        let main = snapshot.primary_window().unwrap();
        assert_eq!(main.id, "rolling");
        assert_eq!(main.label, "5h limit");
        // `percent` is used, not remaining, so no inversion.
        assert!((main.used_fraction - 0.425).abs() < 1e-9);
        assert_eq!(snapshot.account.unwrap().plan.as_deref(), Some("Go"));
    }

    #[test]
    fn a_reset_with_milliseconds_parses() {
        let snapshot = parse(OK).unwrap();
        assert_eq!(
            snapshot.windows[0].resets_at,
            Some(at("2026-09-08T17:00:00.000Z"))
        );
    }

    #[test]
    fn a_percent_arriving_as_an_integer_parses_too() {
        let snapshot = parse(OK).unwrap();
        let weekly = snapshot.windows.iter().find(|w| w.id == "weekly").unwrap();
        assert!((weekly.used_fraction - 0.73).abs() < 1e-9);
    }

    #[test]
    fn a_window_without_a_percent_is_skipped_rather_than_counted_as_zero() {
        let body = r#"{"usage":{"rolling":{"percent":10},"weekly":{"status":"ok"}}}"#;
        let snapshot = parse(body).unwrap();
        let ids: Vec<&str> = snapshot.windows.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, ["rolling"]);
    }

    #[test]
    fn an_unknown_field_in_a_window_is_ignored() {
        // `status` is present in every real response and read by nothing.
        let body = r#"{"usage":{"rolling":{"status":"ok","percent":10}}}"#;
        assert!(parse(body).is_ok());
    }

    #[test]
    fn an_empty_usage_object_is_nothing_metered_not_zero() {
        assert!(matches!(
            parse(r#"{"usage":{}}"#),
            Err(ProviderError::NothingMetered(_))
        ));
    }

    #[test]
    fn a_body_without_usage_is_a_bad_response() {
        assert!(matches!(parse("{}"), Err(ProviderError::BadResponse { status: 200 })));
        assert!(matches!(parse("not json"), Err(ProviderError::BadResponse { status: 200 })));
    }

    #[test]
    fn the_go_key_parses_in_both_shapes_it_has_shipped_in() {
        let wrapped = r#"{"opencode-go":{"type":"api","key":"token-value"}}"#;
        assert_eq!(parse_credential(wrapped).unwrap(), "token-value");

        let bare = r#"{"opencode-go":"token-value"}"#;
        assert_eq!(parse_credential(bare).unwrap(), "token-value");

        let alternative = r#"{"opencode-go":{"apiKey":"token-value"}}"#;
        assert_eq!(parse_credential(alternative).unwrap(), "token-value");
    }

    #[test]
    fn another_vendors_key_in_the_same_file_is_never_claimed() {
        // Claiming this would report an OpenAI account under OpenCode's name.
        let raw = r#"{"openai":{"key":"someone-elses-key"},"google":{"key":"another"}}"#;
        assert!(matches!(parse_credential(raw), Err(ProviderError::NeedsAuth)));
    }

    #[test]
    fn an_empty_or_missing_key_counts_as_signed_out() {
        assert!(matches!(
            parse_credential(r#"{"opencode-go":{"key":"  "}}"#),
            Err(ProviderError::NeedsAuth)
        ));
        assert!(matches!(
            parse_credential(r#"{"opencode-go":{"type":"api"}}"#),
            Err(ProviderError::NeedsAuth)
        ));
        assert!(matches!(parse_credential("{}"), Err(ProviderError::NeedsAuth)));
    }

    #[test]
    fn a_malformed_file_is_an_io_error_without_echoing_it() {
        let error = parse_credential("{ not json").unwrap_err();
        assert!(matches!(error, ProviderError::Io(_)));
    }
}
