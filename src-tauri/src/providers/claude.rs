//! Claude Code adapter (SPEC §7.1) — `Fidelity::Official`.
//!
//! Reads the credential Claude Code already left on the machine and asks
//! Anthropic's own usage endpoint. The endpoint is **undocumented**: any parse
//! failure becomes [`ProviderError::BadResponse`], never a guessed number.
//!
//! The credential is read-only in every sense: we open it, we parse it, we never
//! write it, and its contents never reach a log line.

use std::sync::RwLock;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::model::{
    sort_windows, Fidelity, LimitWindow, ProviderAccount, ProviderError, ProviderId,
    ProviderSnapshot, SignInHint, SESSION_WINDOW_ID,
};
use crate::providers::{retry_after_header, UsageProvider};

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const BETA_HEADER: &str = "anthropic-beta";
const BETA_VALUE: &str = "oauth-2025-04-20";
const TIMEOUT: Duration = Duration::from_secs(15);
const MANAGE_URL: &str = "https://claude.ai/settings/usage";
/// Keychain service that Claude Code stores its credential under on macOS.
#[cfg(target_os = "macos")]
const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";

// ---------------------------------------------------------------------------
// Credential
// ---------------------------------------------------------------------------

/// Key that holds the Claude.ai subscription login. The same file may also carry
/// unrelated MCP server tokens, which this adapter ignores entirely.
const OAUTH_KEY: &str = "claudeAiOauth";

#[derive(Debug, Deserialize)]
struct CredentialFile {
    #[serde(rename = "claudeAiOauth")]
    claude_ai_oauth: OauthCredential,
}

#[derive(Debug, Clone, Deserialize)]
struct OauthCredential {
    #[serde(rename = "accessToken")]
    access_token: String,
    /// Milliseconds since the epoch.
    #[serde(rename = "expiresAt")]
    expires_at: Option<i64>,
    #[serde(rename = "subscriptionType")]
    subscription_type: Option<String>,
}

impl OauthCredential {
    fn is_expired(&self) -> bool {
        self.expires_at.is_some_and(|at| at <= Utc::now().timestamp_millis())
    }
}

/// Parses the credential JSON. The error text describes the *shape* problem and
/// never echoes the file contents.
fn parse_credential(raw: &str) -> Result<OauthCredential, ProviderError> {
    // The same file also holds MCP server tokens under `mcpOAuth`, so a file
    // without `claudeAiOauth` is not corrupt — it just means Claude Code has no
    // subscription login stored here. That is `NeedsAuth`, not an error.
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(value) if value.get(OAUTH_KEY).is_none() => {
            log_credential_shape(raw);
            return Err(ProviderError::NeedsAuth);
        }
        Ok(_) => {}
        Err(_) => {
            log_credential_shape(raw);
            return Err(ProviderError::Io("credential file is not valid json".into()));
        }
    }

    let file: CredentialFile = match serde_json::from_str(raw) {
        Ok(file) => file,
        Err(error) => {
            log_credential_shape(raw);
            return Err(ProviderError::Io(format!(
                "credential {OAUTH_KEY} is not the expected shape ({})",
                error.classify_str()
            )));
        }
    };
    if file.claude_ai_oauth.access_token.is_empty() {
        return Err(ProviderError::NeedsAuth);
    }
    Ok(file.claude_ai_oauth)
}

/// Diagnostic for when Anthropic changes the credential layout: logs the key
/// **names** and their JSON types, never a value. Enough to fix the parser
/// without anyone having to paste a token anywhere.
///
/// Once per process: the credential is re-read on every poll and on every
/// `get_state`, and repeating this would bury everything else in the log.
fn log_credential_shape(raw: &str) {
    static LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if LOGGED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        return;
    }

    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw) else {
        tracing::debug!("credential file is not valid json");
        return;
    };

    fn type_of(value: &serde_json::Value) -> &'static str {
        match value {
            serde_json::Value::Null => "null",
            serde_json::Value::Bool(_) => "bool",
            serde_json::Value::Number(_) => "number",
            serde_json::Value::String(_) => "string",
            serde_json::Value::Array(_) => "array",
            serde_json::Value::Object(_) => "object",
        }
    }

    fn keys(value: &serde_json::Value) -> Vec<String> {
        value
            .as_object()
            .map(|map| map.iter().map(|(key, value)| format!("{key}: {}", type_of(value))).collect())
            .unwrap_or_default()
    }

    tracing::debug!(shape = ?keys(&value), "credential top level");
    for (key, nested) in value.as_object().into_iter().flatten() {
        if nested.is_object() {
            tracing::debug!(object = %key, shape = ?keys(nested), "credential nested object");
        }
    }
}

/// Small helper so the error message says *why* without quoting the file.
trait ClassifyStr {
    fn classify_str(&self) -> &'static str;
}

impl ClassifyStr for serde_json::Error {
    fn classify_str(&self) -> &'static str {
        match self.classify() {
            serde_json::error::Category::Io => "io",
            serde_json::error::Category::Syntax => "not valid json",
            serde_json::error::Category::Data => "missing or mistyped field",
            serde_json::error::Category::Eof => "truncated",
        }
    }
}

/// `~/.claude/.credentials.json` — plain text on Windows and Linux; on macOS the
/// same JSON lives in the Keychain instead.
#[cfg(not(target_os = "macos"))]
fn read_credential() -> Result<OauthCredential, ProviderError> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
    let Some(home) = home else {
        return Err(ProviderError::Io("neither USERPROFILE nor HOME is set".into()));
    };
    let path = std::path::PathBuf::from(home).join(".claude").join(".credentials.json");
    let raw = match std::fs::read_to_string(&path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ProviderError::NeedsAuth)
        }
        Err(error) => return Err(ProviderError::Io(error.to_string())),
    };
    parse_credential(&raw)
}

#[cfg(target_os = "macos")]
fn read_credential() -> Result<OauthCredential, ProviderError> {
    use security_framework::item::{ItemClass, ItemSearchOptions, Limit, SearchResult};

    let mut options = ItemSearchOptions::new();
    let results = options
        .class(ItemClass::generic_password())
        .service(KEYCHAIN_SERVICE)
        .limit(Limit::All)
        .load_data(true)
        .search()
        .map_err(|_| ProviderError::NeedsAuth)?;

    // Claude Code can leave several items behind; keep the one that expires last,
    // which is the most recently refreshed one (SPEC §7.1).
    let mut newest: Option<OauthCredential> = None;
    for result in results {
        let SearchResult::Data(bytes) = result else { continue };
        let Ok(text) = String::from_utf8(bytes) else { continue };
        let Ok(candidate) = parse_credential(&text) else { continue };
        let is_newer = newest
            .as_ref()
            .is_none_or(|best| candidate.expires_at.unwrap_or(0) > best.expires_at.unwrap_or(0));
        if is_newer {
            newest = Some(candidate);
        }
    }
    newest.ok_or(ProviderError::NeedsAuth)
}

// ---------------------------------------------------------------------------
// Response
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct UsageResponse {
    #[serde(default)]
    limits: Vec<LimitEntry>,
    /// The session window, also exposed under its own name.
    #[serde(default)]
    five_hour: Option<NamedWindow>,
    #[serde(default)]
    seven_day: Option<NamedWindow>,
}

#[derive(Debug, Deserialize)]
struct LimitEntry {
    kind: String,
    /// 0–100. Absent means the window is absent, not zero.
    #[serde(default)]
    percent: Option<f64>,
    #[serde(default)]
    resets_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct NamedWindow {
    #[serde(default)]
    utilization: Option<f64>,
    #[serde(default)]
    resets_at: Option<DateTime<Utc>>,
}

fn fraction(percent: f64) -> f64 {
    (percent / 100.0).clamp(0.0, 1.0)
}

fn label_for(kind: &str) -> String {
    match kind {
        SESSION_WINDOW_ID => "Session (5h)".to_string(),
        "weekly_all" => "Weekly (all models)".to_string(),
        "weekly_opus" => "Weekly (Opus)".to_string(),
        "weekly_sonnet" => "Weekly (Sonnet)".to_string(),
        // Anthropic adds kinds over time; show a readable label instead of hiding
        // the window.
        other => other
            .split('_')
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// `limits[]` is the source of truth; `five_hour`/`seven_day` fill in a window
/// that momentarily disappears from the array right after it resets (SPEC §7.1).
fn parse_usage(
    body: &str,
    fetched_at: DateTime<Utc>,
    account: Option<ProviderAccount>,
) -> Result<ProviderSnapshot, ProviderError> {
    let response: UsageResponse =
        serde_json::from_str(body).map_err(|_| ProviderError::BadResponse { status: 200 })?;

    let mut windows: Vec<LimitWindow> = response
        .limits
        .iter()
        .filter_map(|entry| {
            entry.percent.map(|percent| LimitWindow {
                id: entry.kind.clone(),
                label: label_for(&entry.kind),
                used_fraction: fraction(percent),
                resets_at: entry.resets_at,
            })
        })
        .collect();

    let mut merge = |id: &str, named: Option<NamedWindow>| {
        if windows.iter().any(|window| window.id == id) {
            return;
        }
        let Some(named) = named else { return };
        let Some(utilization) = named.utilization else { return };
        windows.push(LimitWindow {
            id: id.to_string(),
            label: label_for(id),
            used_fraction: fraction(utilization),
            resets_at: named.resets_at,
        });
    };
    merge(SESSION_WINDOW_ID, response.five_hour);
    merge("weekly_all", response.seven_day);

    if windows.is_empty() {
        return Err(ProviderError::NothingMetered(
            "usage response carried no limit window".into(),
        ));
    }

    sort_windows(&mut windows);
    Ok(ProviderSnapshot {
        provider: ProviderId::Claude,
        fidelity: Fidelity::Official,
        windows,
        fetched_at,
        account,
    })
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

pub struct ClaudeProvider {
    client: reqwest::Client,
    /// Held only for the lifetime of the process; never persisted (SPEC §11).
    cached: RwLock<Option<OauthCredential>>,
}

impl ClaudeProvider {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .user_agent(concat!("GaugeCode/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client, cached: RwLock::new(None) }
    }

    fn credential(&self, force_reload: bool) -> Result<OauthCredential, ProviderError> {
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

    async fn request(&self, credential: &OauthCredential) -> Result<ProviderSnapshot, ProviderError> {
        let started = Instant::now();
        let response = self
            .client
            .get(USAGE_URL)
            .bearer_auth(&credential.access_token)
            .header(BETA_HEADER, BETA_VALUE)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|error| ProviderError::Io(error.without_url().to_string()))?;

        let status = response.status();
        // Status, provider and duration only — never a header or a body.
        tracing::debug!(
            provider = "claude",
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
                let account = Some(ProviderAccount {
                    email: None,
                    plan: credential.subscription_type.clone(),
                });
                parse_usage(&body, Utc::now(), account)
            }
            401 | 403 => Err(ProviderError::AccessDenied),
            429 => Err(ProviderError::RateLimited {
                retry_after: retry_after_header(response.headers()),
            }),
            other => Err(ProviderError::BadResponse { status: other }),
        }
    }
}

impl Default for ClaudeProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl UsageProvider for ClaudeProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Claude
    }

    fn fidelity(&self) -> Fidelity {
        Fidelity::Official
    }

    async fn fetch_snapshot(&self) -> Result<ProviderSnapshot, ProviderError> {
        let credential = self.credential(false)?;
        match self.request(&credential).await {
            Err(ProviderError::AccessDenied) => {
                // Claude Code refreshes the token in place, so a 401 usually just
                // means our cached copy is stale. Re-read from disk exactly once.
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
        let credential = self.credential(false).ok()?;
        Some(ProviderAccount { email: None, plan: credential.subscription_type })
    }

    fn forget_cached_credential(&self) {
        *self.cached.write().unwrap_or_else(|e| e.into_inner()) = None;
    }

    fn sign_in_hint(&self) -> SignInHint {
        SignInHint::Text {
            text: "Run `claude` in a terminal and sign in; GaugeCode reads the credential it leaves behind."
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

    const OK: &str = include_str!("../../tests/fixtures/claude_usage_ok.json");
    const JUST_RESET: &str = include_str!("../../tests/fixtures/claude_usage_just_reset.json");
    const EMPTY: &str = include_str!("../../tests/fixtures/claude_usage_empty.json");
    const CREDENTIAL: &str = include_str!("../../tests/fixtures/claude_credential.json");

    fn parse(body: &str) -> Result<ProviderSnapshot, ProviderError> {
        parse_usage(body, Utc::now(), None)
    }

    #[test]
    fn parses_the_documented_shape_with_session_first() {
        let snapshot = parse(OK).expect("fixture should parse");
        assert_eq!(snapshot.fidelity, Fidelity::Official);
        let ids: Vec<&str> = snapshot.windows.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, ["session", "weekly_all", "weekly_opus"]);

        let session = snapshot.primary_window().expect("session window");
        assert_eq!(session.label, "Session (5h)");
        assert!((session.used_fraction - 0.42).abs() < 1e-9);
        assert!(session.resets_at.is_some());
    }

    #[test]
    fn merges_five_hour_when_session_vanished_from_the_array() {
        let snapshot = parse(JUST_RESET).expect("fixture should parse");
        let session = snapshot.primary_window().expect("session merged from five_hour");
        assert_eq!(session.id, "session");
        assert!((session.used_fraction - 0.03).abs() < 1e-9);
        // weekly_all is already in limits[], so seven_day must not duplicate it.
        assert_eq!(snapshot.windows.iter().filter(|w| w.id == "weekly_all").count(), 1);
        assert!((snapshot.windows.iter().find(|w| w.id == "weekly_all").unwrap().used_fraction
            - 0.615)
            .abs()
            < 1e-9);
    }

    #[test]
    fn a_response_with_no_window_is_nothing_metered_not_zero() {
        assert!(matches!(parse(EMPTY), Err(ProviderError::NothingMetered(_))));
    }

    #[test]
    fn garbage_is_a_bad_response_never_a_number() {
        assert!(matches!(parse("not json"), Err(ProviderError::BadResponse { status: 200 })));
        assert!(matches!(
            parse(r#"{"limits":"nope"}"#),
            Err(ProviderError::BadResponse { status: 200 })
        ));
    }

    #[test]
    fn an_entry_without_percent_is_skipped_rather_than_counted_as_zero() {
        let body = r#"{"limits":[{"kind":"session","percent":50},{"kind":"weekly_opus"}]}"#;
        let snapshot = parse(body).expect("session still parses");
        let ids: Vec<&str> = snapshot.windows.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, ["session"]);
    }

    #[test]
    fn percentages_are_clamped_into_the_unit_range() {
        let body = r#"{"limits":[{"kind":"session","percent":140},{"kind":"weekly_all","percent":-5}]}"#;
        let snapshot = parse(body).expect("parses");
        assert_eq!(snapshot.windows[0].used_fraction, 1.0);
        assert_eq!(snapshot.windows[1].used_fraction, 0.0);
    }

    #[test]
    fn unknown_kinds_get_a_readable_label() {
        assert_eq!(label_for("session"), "Session (5h)");
        assert_eq!(label_for("weekly_opus"), "Weekly (Opus)");
        assert_eq!(label_for("weekly_haiku"), "Weekly Haiku");
        assert_eq!(label_for("something_brand_new"), "Something Brand New");
    }

    #[test]
    fn credential_fixture_parses_and_reports_the_plan() {
        let credential = parse_credential(CREDENTIAL).expect("fixture should parse");
        assert_eq!(credential.subscription_type.as_deref(), Some("max"));
        assert!(!credential.access_token.is_empty());
    }

    #[test]
    fn a_file_holding_only_mcp_tokens_means_needs_auth_not_an_error() {
        // Real case on the author's machine: `.credentials.json` existed but only
        // carried MCP server tokens, with no Claude.ai login at all.
        let only_mcp = r#"{"mcpOAuth":{"github|abc":{"accessToken":"secret-value"}}}"#;
        assert!(matches!(parse_credential(only_mcp), Err(ProviderError::NeedsAuth)));
    }

    #[test]
    fn a_malformed_oauth_block_is_an_io_error_without_echoing_the_file() {
        let error =
            parse_credential(r#"{"claudeAiOauth":{"accessToken":12,"note":"secret-value"}}"#)
                .unwrap_err();
        let message = error.to_string();
        assert!(matches!(error, ProviderError::Io(_)));
        assert!(!message.contains("secret-value"), "error text must not echo the file");
    }

    #[test]
    fn invalid_json_is_reported_as_such() {
        assert!(matches!(parse_credential("{ not json"), Err(ProviderError::Io(_))));
    }

    #[test]
    fn expiry_is_read_from_the_credential() {
        let expired = parse_credential(
            r#"{"claudeAiOauth":{"accessToken":"t","expiresAt":1000,"subscriptionType":"pro"}}"#,
        )
        .unwrap();
        assert!(expired.is_expired());

        let valid = parse_credential(
            r#"{"claudeAiOauth":{"accessToken":"t","expiresAt":99999999999999}}"#,
        )
        .unwrap();
        assert!(!valid.is_expired());
    }

}
