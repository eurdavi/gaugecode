//! Codex adapter (SPEC §7.3) — `Fidelity::Official`.
//!
//! Reads `~/.codex/auth.json`, which the Codex CLI writes when you sign in, and
//! asks ChatGPT's usage endpoint.
//!
//! ⚠️ **The response shape here is the least verified of the three adapters.**
//! Nobody on this project has a Codex login to capture a real body from, so the
//! field names below are accepted under several spellings via `serde(alias)`.
//! Anything the parser does not recognise becomes [`ProviderError::BadResponse`]
//! — never a guessed percentage. See SPEC §14.

use std::path::PathBuf;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::model::{
    sort_windows, Fidelity, LimitWindow, ProviderAccount, ProviderError, ProviderId,
    ProviderSnapshot, SignInHint,
};
use crate::providers::UsageProvider;

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const ACCOUNT_HEADER: &str = "ChatGPT-Account-Id";
const TIMEOUT: Duration = Duration::from_secs(15);
const MANAGE_URL: &str = "https://chatgpt.com/#settings/Account";

// ---------------------------------------------------------------------------
// Credential
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct AuthFile {
    tokens: Option<Tokens>,
}

#[derive(Debug, Clone, Deserialize)]
struct Tokens {
    access_token: String,
    #[serde(default)]
    account_id: Option<String>,
}

#[derive(Debug, Clone)]
struct Credential {
    access_token: String,
    account_id: Option<String>,
    /// Read from the token's own `exp` claim, so an expired login is reported as
    /// expired instead of as a mysterious 401.
    expires_at: Option<DateTime<Utc>>,
    email: Option<String>,
    plan: Option<String>,
}

impl Credential {
    fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|at| at <= Utc::now())
    }
}

/// Minimal base64url decoder. A JWT payload is base64url without padding, and
/// pulling in a base64 crate for twenty lines is not worth the dependency
/// (AGENTS.md keeps the crate list short).
fn decode_base64url(input: &str) -> Option<Vec<u8>> {
    fn sextet(byte: u8) -> Option<u32> {
        Some(match byte {
            b'A'..=b'Z' => (byte - b'A') as u32,
            b'a'..=b'z' => (byte - b'a') as u32 + 26,
            b'0'..=b'9' => (byte - b'0') as u32 + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        })
    }

    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for byte in input.bytes() {
        if byte == b'=' {
            break;
        }
        buffer = (buffer << 6) | sextet(byte)?;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Some(out)
}

/// The `exp` and profile claims of a JWT. The signature is deliberately **not**
/// checked: this is not an authorisation decision, it only decides whether to
/// say "expired" instead of "denied", and the server is the one that validates.
fn claims_of(token: &str) -> Option<serde_json::Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = decode_base64url(payload)?;
    serde_json::from_slice(&bytes).ok()
}

/// Pulls whatever profile fields the token happens to carry. All optional: they
/// only decorate the Settings row.
fn profile_of(claims: &serde_json::Value) -> (Option<String>, Option<String>) {
    let string_at = |path: &[&str]| -> Option<String> {
        let mut value = claims;
        for key in path {
            value = value.get(key)?;
        }
        value.as_str().map(str::to_string)
    };

    let email = string_at(&["email"])
        .or_else(|| string_at(&["https://api.openai.com/profile", "email"]));
    let plan = string_at(&["https://api.openai.com/auth", "chatgpt_plan_type"]);
    (email, plan)
}

fn parse_credential(raw: &str) -> Result<Credential, ProviderError> {
    let file: AuthFile = serde_json::from_str(raw)
        .map_err(|_| ProviderError::Io("auth.json is not the expected shape".into()))?;

    // A file with no `tokens` block means the CLI is installed but not signed
    // in. That is `NeedsAuth`, not a parse failure.
    let Some(tokens) = file.tokens.filter(|tokens| !tokens.access_token.is_empty()) else {
        return Err(ProviderError::NeedsAuth);
    };

    let claims = claims_of(&tokens.access_token);
    let expires_at = claims
        .as_ref()
        .and_then(|claims| claims.get("exp"))
        .and_then(serde_json::Value::as_i64)
        .and_then(|seconds| DateTime::from_timestamp(seconds, 0));
    let (email, plan) = claims.as_ref().map(profile_of).unwrap_or((None, None));

    Ok(Credential {
        access_token: tokens.access_token,
        account_id: tokens.account_id,
        expires_at,
        email,
        plan,
    })
}

fn auth_path() -> Result<PathBuf, ProviderError> {
    // `USERPROFILE` on Windows, `HOME` everywhere else — both mean `~`.
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
    let Some(home) = home else {
        return Err(ProviderError::Io("neither USERPROFILE nor HOME is set".into()));
    };
    Ok(PathBuf::from(home).join(".codex").join("auth.json"))
}

fn read_credential() -> Result<Credential, ProviderError> {
    let path = auth_path()?;
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        // The Codex CLI is not installed, or was never signed in.
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
    #[serde(default, alias = "rate_limits")]
    rate_limit: Option<RateLimit>,
}

/// `additional_rate_limits` and `code_review_rate_limit` are deliberately
/// ignored: they meter something else (SPEC §7.3).
#[derive(Debug, Deserialize)]
struct RateLimit {
    #[serde(default, alias = "primary")]
    primary_window: Option<Window>,
    #[serde(default, alias = "secondary")]
    secondary_window: Option<Window>,
}

#[derive(Debug, Deserialize)]
struct Window {
    /// 0–100. Absent means the window is absent, not zero.
    #[serde(default, alias = "percent", alias = "utilization")]
    used_percent: Option<f64>,
    /// Length of the window; only used to build a readable label.
    #[serde(default)]
    window_minutes: Option<i64>,
    #[serde(default)]
    resets_at: Option<DateTime<Utc>>,
    #[serde(default, alias = "resets_in_seconds")]
    reset_after_seconds: Option<i64>,
}

impl Window {
    fn resets_at(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.resets_at
            .or_else(|| self.reset_after_seconds.map(|secs| now + chrono::Duration::seconds(secs)))
    }
}

/// `300` minutes reads better as "5h" than as "300m"; anything we cannot name
/// keeps a generic label rather than being hidden.
fn label_for(fallback: &str, window_minutes: Option<i64>) -> String {
    match window_minutes {
        Some(minutes) if minutes % (60 * 24 * 7) == 0 => "Weekly limit".into(),
        Some(minutes) if minutes % (60 * 24) == 0 => {
            format!("{}-day limit", minutes / (60 * 24))
        }
        Some(minutes) if minutes % 60 == 0 => format!("{}h limit", minutes / 60),
        Some(minutes) => format!("{minutes}m limit"),
        None => fallback.into(),
    }
}

fn fraction(percent: f64) -> f64 {
    (percent / 100.0).clamp(0.0, 1.0)
}

fn parse_usage(
    body: &str,
    fetched_at: DateTime<Utc>,
    account: Option<ProviderAccount>,
) -> Result<ProviderSnapshot, ProviderError> {
    let response: UsageResponse =
        serde_json::from_str(body).map_err(|_| ProviderError::BadResponse { status: 200 })?;

    let rate_limit = response.rate_limit.unwrap_or(RateLimit {
        primary_window: None,
        secondary_window: None,
    });

    let mut windows = Vec::new();
    let mut push = |id: &str, fallback: &str, window: Option<Window>| {
        let Some(window) = window else { return };
        let Some(percent) = window.used_percent else { return };
        windows.push(LimitWindow {
            id: id.to_string(),
            label: label_for(fallback, window.window_minutes),
            used_fraction: fraction(percent),
            resets_at: window.resets_at(fetched_at),
        });
    };
    push("primary", "Primary limit", rate_limit.primary_window);
    push("secondary", "Secondary limit", rate_limit.secondary_window);

    if windows.is_empty() {
        return Err(ProviderError::NothingMetered(
            "usage response carried no rate limit window".into(),
        ));
    }

    sort_windows(&mut windows);
    Ok(ProviderSnapshot {
        provider: ProviderId::Codex,
        fidelity: Fidelity::Official,
        windows,
        fetched_at,
        account,
    })
}

fn retry_after(headers: &reqwest::header::HeaderMap) -> Duration {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::ZERO)
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

pub struct CodexProvider {
    client: reqwest::Client,
    /// Process lifetime only; never persisted (SPEC §11).
    cached: RwLock<Option<Credential>>,
}

impl CodexProvider {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .user_agent(concat!("GaugeCode/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client, cached: RwLock::new(None) }
    }

    fn credential(&self, force_reload: bool) -> Result<Credential, ProviderError> {
        if !force_reload {
            let cached = self.cached.read().unwrap_or_else(|e| e.into_inner());
            if let Some(credential) = cached.as_ref() {
                return Ok(credential.clone());
            }
        }
        let credential = read_credential()?;
        *self.cached.write().unwrap_or_else(|e| e.into_inner()) = Some(credential.clone());
        Ok(credential)
    }

    fn account_of(credential: &Credential) -> Option<ProviderAccount> {
        Some(ProviderAccount { email: credential.email.clone(), plan: credential.plan.clone() })
    }

    async fn request(&self, credential: &Credential) -> Result<ProviderSnapshot, ProviderError> {
        let started = Instant::now();
        let mut request = self
            .client
            .get(USAGE_URL)
            .bearer_auth(&credential.access_token)
            .header(reqwest::header::ACCEPT, "application/json");
        if let Some(account_id) = credential.account_id.as_deref() {
            request = request.header(ACCOUNT_HEADER, account_id);
        }

        let response = request
            .send()
            .await
            .map_err(|error| ProviderError::Io(error.without_url().to_string()))?;

        let status = response.status();
        // Status, provider and duration only — never a header or a body.
        tracing::debug!(
            provider = "codex",
            status = status.as_u16(),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "usage request finished"
        );

        match status.as_u16() {
            200 => {
                let body = response
                    .text()
                    .await
                    .map_err(|error| ProviderError::Io(error.without_url().to_string()))?;
                parse_usage(&body, Utc::now(), Self::account_of(credential))
            }
            401 | 403 => Err(ProviderError::AccessDenied),
            429 => Err(ProviderError::RateLimited { retry_after: retry_after(response.headers()) }),
            other => Err(ProviderError::BadResponse { status: other }),
        }
    }
}

impl Default for CodexProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl UsageProvider for CodexProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Codex
    }

    fn fidelity(&self) -> Fidelity {
        Fidelity::Official
    }

    async fn fetch_snapshot(&self) -> Result<ProviderSnapshot, ProviderError> {
        let credential = self.credential(false)?;
        match self.request(&credential).await {
            Err(ProviderError::AccessDenied) => {
                // The CLI refreshes the token in place, so re-read from disk once
                // before concluding the login is gone.
                self.forget_cached_credential();
                let credential = self.credential(true)?;
                match self.request(&credential).await {
                    Err(ProviderError::AccessDenied) if credential.is_expired() => {
                        Err(ProviderError::CredentialExpired)
                    }
                    Err(ProviderError::AccessDenied) => Err(ProviderError::NeedsAuth),
                    other => other,
                }
            }
            other => other,
        }
    }

    fn account(&self) -> Option<ProviderAccount> {
        Self::account_of(&self.credential(false).ok()?)
    }

    fn forget_cached_credential(&self) {
        *self.cached.write().unwrap_or_else(|e| e.into_inner()) = None;
    }

    fn sign_in_hint(&self) -> SignInHint {
        SignInHint::Text {
            text: "Install the Codex CLI and run `codex` to sign in; GaugeCode reads the credential it leaves behind."
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

    const OK: &str = include_str!("../../tests/fixtures/codex_usage_ok.json");
    const RELATIVE: &str = include_str!("../../tests/fixtures/codex_usage_relative_reset.json");
    const EMPTY: &str = include_str!("../../tests/fixtures/codex_usage_empty.json");

    fn at(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text).unwrap().with_timezone(&Utc)
    }

    fn parse(body: &str) -> Result<ProviderSnapshot, ProviderError> {
        parse_usage(body, at("2026-09-07T12:00:00Z"), None)
    }

    #[test]
    fn parses_both_windows_in_order() {
        let snapshot = parse(OK).expect("fixture should parse");
        assert_eq!(snapshot.provider, ProviderId::Codex);
        let ids: Vec<&str> = snapshot.windows.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, ["primary", "secondary"]);

        // Codex has no `session` window, so the ring shows the primary one.
        let main = snapshot.primary_window().unwrap();
        assert_eq!(main.id, "primary");
        assert!((main.used_fraction - 0.234).abs() < 1e-9);
        assert_eq!(main.label, "5h limit");
        assert_eq!(snapshot.windows[1].label, "Weekly limit");
    }

    #[test]
    fn a_relative_reset_becomes_an_absolute_one() {
        let snapshot = parse(RELATIVE).expect("fixture should parse");
        // 10 800 s after the fetch time.
        assert_eq!(snapshot.windows[0].resets_at, Some(at("2026-09-07T15:00:00Z")));
    }

    #[test]
    fn the_alternative_field_spellings_parse_too() {
        // Same data under `rate_limits`/`primary`/`percent`, which is how some
        // Codex builds spell it. Neither shape is documented (SPEC §7.3).
        let body = r#"{"rate_limits":{"primary":{"percent":50,"window_minutes":300}}}"#;
        let snapshot = parse(body).expect("aliases should parse");
        assert_eq!(snapshot.windows[0].used_fraction, 0.5);
        assert_eq!(snapshot.windows[0].label, "5h limit");
    }

    #[test]
    fn metering_we_were_told_to_ignore_stays_ignored() {
        let body = r#"{"rate_limit":{"primary_window":{"used_percent":10},
            "additional_rate_limits":[{"used_percent":99}],
            "code_review_rate_limit":{"used_percent":99}}}"#;
        let snapshot = parse(body).expect("primary still parses");
        assert_eq!(snapshot.windows.len(), 1);
        assert_eq!(snapshot.windows[0].used_fraction, 0.1);
    }

    #[test]
    fn a_window_without_a_percent_is_skipped_rather_than_counted_as_zero() {
        let body = r#"{"rate_limit":{"primary_window":{"used_percent":10},
            "secondary_window":{"window_minutes":10080}}}"#;
        let snapshot = parse(body).unwrap();
        let ids: Vec<&str> = snapshot.windows.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, ["primary"]);
    }

    #[test]
    fn a_response_with_no_window_is_nothing_metered_not_zero() {
        assert!(matches!(parse(EMPTY), Err(ProviderError::NothingMetered(_))));
        assert!(matches!(parse("{}"), Err(ProviderError::NothingMetered(_))));
    }

    #[test]
    fn garbage_is_a_bad_response_never_a_number() {
        assert!(matches!(parse("not json"), Err(ProviderError::BadResponse { status: 200 })));
        assert!(matches!(
            parse(r#"{"rate_limit":7}"#),
            Err(ProviderError::BadResponse { status: 200 })
        ));
    }

    #[test]
    fn window_labels_are_readable_for_the_lengths_codex_uses() {
        assert_eq!(label_for("x", Some(300)), "5h limit");
        assert_eq!(label_for("x", Some(60 * 24 * 7)), "Weekly limit");
        assert_eq!(label_for("x", Some(60 * 24 * 3)), "3-day limit");
        assert_eq!(label_for("x", Some(45)), "45m limit");
        assert_eq!(label_for("Primary limit", None), "Primary limit");
    }

    #[test]
    fn base64url_decodes_without_padding() {
        // "{"exp":1}" — the shape a JWT payload arrives in.
        assert_eq!(decode_base64url("eyJleHAiOjF9").unwrap(), br#"{"exp":1}"#);
        assert_eq!(decode_base64url("-_8").unwrap(), [0xfb, 0xff]);
        assert!(decode_base64url("not base64!").is_none());
    }

    /// Header `{"alg":"none"}`, payload with an `exp` and a plan, empty signature.
    fn token_with(payload: &str) -> String {
        fn encode(bytes: &[u8]) -> String {
            const ALPHABET: &[u8] =
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
            let mut out = String::new();
            for chunk in bytes.chunks(3) {
                let mut buffer = [0u8; 3];
                buffer[..chunk.len()].copy_from_slice(chunk);
                let value = u32::from_be_bytes([0, buffer[0], buffer[1], buffer[2]]);
                let sextets = [(value >> 18) & 63, (value >> 12) & 63, (value >> 6) & 63, value & 63];
                for sextet in sextets.iter().take(chunk.len() + 1) {
                    out.push(ALPHABET[*sextet as usize] as char);
                }
            }
            out
        }
        format!("{}.{}.", encode(br#"{"alg":"none"}"#), encode(payload.as_bytes()))
    }

    #[test]
    fn expiry_and_profile_come_from_the_token_claims() {
        let token = token_with(
            r#"{"exp":1000,"email":"me@example.com",
                "https://api.openai.com/auth":{"chatgpt_plan_type":"plus"}}"#,
        );
        let raw = format!(r#"{{"tokens":{{"access_token":"{token}","account_id":"acct_1"}}}}"#);
        let credential = parse_credential(&raw).expect("auth.json should parse");

        assert_eq!(credential.account_id.as_deref(), Some("acct_1"));
        assert_eq!(credential.email.as_deref(), Some("me@example.com"));
        assert_eq!(credential.plan.as_deref(), Some("plus"));
        // exp of 1000 is 1970, so this login is long gone.
        assert!(credential.is_expired());
    }

    #[test]
    fn an_opaque_token_still_works_it_just_has_no_expiry() {
        let raw = r#"{"tokens":{"access_token":"not-a-jwt","account_id":"acct_1"}}"#;
        let credential = parse_credential(raw).expect("auth.json should parse");
        assert_eq!(credential.expires_at, None);
        assert!(!credential.is_expired(), "unknown expiry must not read as expired");
    }

    #[test]
    fn a_file_without_tokens_means_needs_auth_not_an_error() {
        assert!(matches!(parse_credential(r#"{}"#), Err(ProviderError::NeedsAuth)));
        assert!(matches!(
            parse_credential(r#"{"tokens":{"access_token":""}}"#),
            Err(ProviderError::NeedsAuth)
        ));
    }

    #[test]
    fn a_malformed_file_is_an_io_error_without_echoing_it() {
        let error = parse_credential(r#"{"tokens":{"access_token":42,"note":"secret-value"}}"#)
            .unwrap_err();
        assert!(matches!(error, ProviderError::Io(_)));
        assert!(!error.to_string().contains("secret-value"));
        assert!(matches!(parse_credential("{ not json"), Err(ProviderError::Io(_))));
    }
}
