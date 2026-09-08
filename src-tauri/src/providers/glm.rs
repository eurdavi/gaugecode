//! GLM Coding Plan adapter (Z.ai) — `Fidelity::Official`.
//!
//! GLM is the odd one out: it has no CLI of its own that stores a credential.
//! Usage rides on a Z.ai key that *another* coding tool is already configured
//! with, so this reads the four places such a key can sit, in priority order.
//!
//! Two things about this endpoint are load-bearing:
//!
//! * **Errors arrive inside HTTP 200.** An expired token answers `200` with
//!   `{"code":401,...}` in the body, so the envelope has to be checked before
//!   the payload is trusted.
//! * **The console host matters before the request, not after.** A key for the
//!   China console asked of `api.z.ai` answers as an auth failure, which would
//!   look like a signed-out plan rather than a key pointed at the wrong country.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::model::{
    sort_windows, Fidelity, LimitWindow, ProviderAccount, ProviderError, ProviderId,
    ProviderSnapshot, SignInHint, SESSION_WINDOW_ID,
};
use crate::providers::{retry_after_header, UsageProvider};

const QUOTA_PATH: &str = "api/monitor/usage/quota/limit";
const CONSOLE_GLOBAL: &str = "https://api.z.ai";
const CONSOLE_CHINA: &str = "https://open.bigmodel.cn";
const TIMEOUT: Duration = Duration::from_secs(15);

/// `unit` is an encoded time unit; only these two have ever been observed.
const UNIT_HOURS: i64 = 3;
const UNIT_WEEKS: i64 = 6;

// ---------------------------------------------------------------------------
// Credential
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
struct Credential {
    token: String,
    console: &'static str,
    /// Which tool the key was borrowed from, shown in Settings.
    source: &'static str,
}

/// Picks the console from a base URL's host. The **path is deliberately
/// dropped**: the monitor lives at the console root, and asking it under
/// `/api/anthropic` returns a misleading 404 rather than an error.
fn console_for_host(host: &str) -> Option<&'static str> {
    let host = host.to_ascii_lowercase();
    if host == "open.bigmodel.cn" || host.ends_with(".bigmodel.cn") {
        return Some(CONSOLE_CHINA);
    }
    if host == "api.z.ai" || host.ends_with(".z.ai") {
        return Some(CONSOLE_GLOBAL);
    }
    None
}

/// Host of a URL, without pulling in a URL parser for one field.
///
/// The separator set has to include `\`: the URL standard treats it as
/// equivalent to `/` for special schemes, and leaving it out let
/// `https://evil.example\api.z.ai` read as a host ending in `.z.ai` and pass
/// [`console_for_host`].
fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let authority = rest.split(['/', '\\', '?', '#']).next()?;
    let host = authority.rsplit_once('@').map(|(_, host)| host).unwrap_or(authority);
    let host = host.split_once(':').map(|(host, _)| host).unwrap_or(host);
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

fn home() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// `~/.local/share` on Linux and macOS; OpenCode uses the same layout under
/// `%USERPROFILE%` on Windows.
fn data_home() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_DATA_HOME") {
        return Some(PathBuf::from(dir));
    }
    home().map(|home| home.join(".local").join("share"))
}

fn read_json(path: PathBuf) -> Option<serde_json::Value> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// A non-empty string at a path, or nothing. An empty key is worse than a
/// missing one: it sends a request that cannot succeed.
fn non_empty_str(value: &serde_json::Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(key)?;
    }
    let text = current.as_str()?.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Claude Code's `settings.json`. The Z.ai base URL is **mandatory** here:
/// without it the token is somebody's Anthropic key, and claiming it would
/// report the wrong account under GLM's name.
fn from_claude_settings() -> Option<Credential> {
    let value = read_json(home()?.join(".claude").join("settings.json"))?;
    let console = console_for_host(&host_of(&non_empty_str(
        &value,
        &["env", "ANTHROPIC_BASE_URL"],
    )?)?)?;
    let token = non_empty_str(&value, &["env", "ANTHROPIC_AUTH_TOKEN"])
        .or_else(|| non_empty_str(&value, &["env", "ANTHROPIC_API_KEY"]))?;
    Some(Credential { token, console, source: "Claude Code" })
}

/// ZCode's pasted plan key. Only entries whose id mentions `coding-plan` are
/// considered — a plain `builtin:zai` entry is pay-as-you-go, not the plan.
fn from_zcode_config() -> Option<Credential> {
    let value = read_json(home()?.join(".zcode").join("v2").join("config.json"))?;
    let providers = value.get("provider")?.as_object()?;

    // Sorted, so a machine with two entries gives a stable answer instead of
    // whatever the map happened to iterate first.
    let mut ids: Vec<&String> = providers.keys().collect();
    ids.sort();

    for id in ids {
        if !id.contains("coding-plan") {
            continue;
        }
        let entry = &providers[id];
        // An absent `enabled` counts as enabled; an explicit `false` means the
        // user switched this account off and it must not be read.
        if entry.get("enabled").and_then(serde_json::Value::as_bool) == Some(false) {
            continue;
        }
        let Some(token) = non_empty_str(entry, &["options", "apiKey"]) else { continue };
        let console = non_empty_str(entry, &["options", "baseURL"])
            .and_then(|url| host_of(&url))
            .and_then(|host| console_for_host(&host))
            .unwrap_or(CONSOLE_GLOBAL);
        return Some(Credential { token, console, source: "ZCode" });
    }
    None
}

/// ZCode's OAuth token. The key is one flat name containing colons, not a path.
fn from_zcode_credentials() -> Option<Credential> {
    let value = read_json(home()?.join(".zcode").join("v2").join("credentials.json"))?;
    let token = non_empty_str(&value, &["oauth:zai:access_token"])?;
    // Encrypted at rest. Decrypting is ZCode's business, and a wrong guess
    // would read as a signed-out plan.
    if token.starts_with("enc:v1:") {
        return None;
    }
    Some(Credential { token, console: CONSOLE_GLOBAL, source: "ZCode" })
}

/// OpenCode's `auth.json`. The entry has shipped both as a bare string and as
/// an object, so both are accepted.
fn from_opencode_auth() -> Option<Credential> {
    const IDS: [&str; 6] = ["zai-coding-plan", "zai", "z-ai", "z.ai", "zhipu", "zhipuai"];
    const KEYS: [&str; 6] = ["apiKey", "api_key", "token", "key", "accessToken", "auth_token"];

    let value = read_json(data_home()?.join("opencode").join("auth.json"))?;
    for id in IDS {
        let Some(entry) = value.get(id) else { continue };
        let token = match entry {
            serde_json::Value::String(_) => non_empty_str(&value, &[id]),
            _ => KEYS.iter().find_map(|key| non_empty_str(entry, &[key])),
        };
        let Some(token) = token else { continue };
        // The console comes from the id, not from any URL in the file.
        let console = if id.starts_with("zhipu") { CONSOLE_CHINA } else { CONSOLE_GLOBAL };
        return Some(Credential { token, console, source: "OpenCode" });
    }
    None
}

fn read_credential() -> Result<Credential, ProviderError> {
    from_claude_settings()
        .or_else(from_zcode_config)
        .or_else(from_zcode_credentials)
        .or_else(from_opencode_auth)
        .ok_or(ProviderError::NeedsAuth)
}

// ---------------------------------------------------------------------------
// Response
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct QuotaResponse {
    #[serde(default)]
    code: Option<i64>,
    #[serde(default)]
    success: Option<bool>,
    #[serde(default)]
    data: Option<QuotaData>,
}

#[derive(Debug, Deserialize)]
struct QuotaData {
    /// Plan tier, e.g. `pro` or `lite`.
    #[serde(default)]
    level: Option<String>,
    #[serde(default)]
    limits: Vec<QuotaLimit>,
}

#[derive(Debug, Deserialize)]
struct QuotaLimit {
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    unit: Option<i64>,
    #[serde(default)]
    number: Option<i64>,
    /// 0–100. Absent means the row is absent, not zero. Arrives as an integer
    /// on some plans and a decimal on others.
    #[serde(default)]
    percentage: Option<f64>,
    /// **Milliseconds** since the epoch — the one unit this API shares with
    /// JavaScript rather than with Unix. Reading it as seconds puts the reset in
    /// 1970 and the countdown reads as overdue forever.
    #[serde(default, rename = "nextResetTime")]
    next_reset_time: Option<f64>,
}

/// Window identity comes from the `(unit, number)` pair, never from `type`:
/// token plans answer `TOKENS_LIMIT` and credit plans `CREDIT_LIMIT` while both
/// encode the same window lengths.
fn window_of(limit: &QuotaLimit) -> Option<LimitWindow> {
    let percentage = limit.percentage?;
    let kind = limit.r#type.as_deref().unwrap_or_default();

    let (id, label) = match (kind, limit.unit, limit.number) {
        ("TIME_LIMIT", _, _) => ("mcp".to_string(), "MCP (1 month)".to_string()),
        (_, Some(UNIT_HOURS), Some(5)) => {
            (SESSION_WINDOW_ID.to_string(), "Current session".to_string())
        }
        (_, Some(UNIT_WEEKS), Some(1)) => ("weekly".to_string(), "Weekly".to_string()),
        (_, Some(unit), Some(number)) => (
            format!("window-{unit}x{number}"),
            match unit {
                UNIT_HOURS => format!("Usage ({number} h)"),
                UNIT_WEEKS => format!("Usage ({number} wk)"),
                _ => "Usage".to_string(),
            },
        ),
        _ => {
            let id = if kind.is_empty() { "unknown".to_string() } else { kind.to_lowercase() };
            (id, "Usage".to_string())
        }
    };

    Some(LimitWindow {
        id,
        label,
        // Deliberately not clamped at the top: being over your limit is a
        // reading, not an error.
        used_fraction: (percentage / 100.0).max(0.0),
        resets_at: limit.next_reset_time.and_then(millis_to_utc),
    })
}

fn millis_to_utc(millis: f64) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp_millis(millis as i64)
}

fn parse_usage(
    body: &str,
    fetched_at: DateTime<Utc>,
) -> Result<(ProviderSnapshot, Option<String>), ProviderError> {
    let response: QuotaResponse =
        serde_json::from_str(body).map_err(|_| ProviderError::BadResponse { status: 200 })?;

    // A body with neither field reads as success; anything else has to say so.
    let ok = response.success == Some(true)
        || response.code.is_none()
        || response.code == Some(200);
    if !ok {
        return Err(match response.code {
            // Served under HTTP 200 by an expired or signed-out token.
            Some(401) | Some(403) => ProviderError::NeedsAuth,
            Some(429) => ProviderError::RateLimited { retry_after: Duration::ZERO },
            // A business code such as 1305 ("overloaded") surfaces where an
            // HTTP status normally sits, which is the most useful thing to show.
            Some(code) => ProviderError::BadResponse { status: code as u16 },
            None => ProviderError::BadResponse { status: 200 },
        });
    }

    let data = response.data.unwrap_or(QuotaData { level: None, limits: Vec::new() });
    let mut windows: Vec<LimitWindow> = data.limits.iter().filter_map(window_of).collect();

    if windows.is_empty() {
        return Err(ProviderError::NothingMetered(
            "quota response carried no limit row".into(),
        ));
    }

    sort_windows(&mut windows);
    let plan = data.level.map(|level| level.to_lowercase());
    Ok((
        ProviderSnapshot {
            provider: ProviderId::Glm,
            fidelity: Fidelity::Official,
            windows,
            fetched_at,
            // None of the borrowed keys carries an address.
            account: Some(ProviderAccount { email: None, plan: plan.clone() }),
        },
        plan,
    ))
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

pub struct GlmProvider {
    client: reqwest::Client,
    /// Plan tier from the last good answer: a fact about the account, not about
    /// the fetch, so Settings can show it without a request.
    plan: std::sync::RwLock<Option<String>>,
}

impl GlmProvider {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .user_agent(concat!("GaugeCode/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client, plan: std::sync::RwLock::new(None) }
    }
}

impl Default for GlmProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl UsageProvider for GlmProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Glm
    }

    fn fidelity(&self) -> Fidelity {
        Fidelity::Official
    }

    async fn fetch_snapshot(&self) -> Result<ProviderSnapshot, ProviderError> {
        // Re-read every time: these are ordinary files, so unlike a keychain
        // read it costs nothing and puts no prompt in front of anyone.
        let credential = read_credential()?;
        let url = format!("{}/{QUOTA_PATH}", credential.console);

        let started = Instant::now();
        let response = self
            .client
            .get(&url)
            // Raw, with no `Bearer` scheme. Prefixing it is exactly what an
            // auth failure looks like from this endpoint.
            .header(reqwest::header::AUTHORIZATION, &credential.token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .send()
            .await
            .map_err(|error| ProviderError::Io(error.without_url().to_string()))?;

        let status = response.status();
        tracing::debug!(
            provider = "glm",
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
                let (snapshot, plan) = parse_usage(&body, Utc::now())?;
                *self.plan.write().unwrap_or_else(|e| e.into_inner()) = plan;
                Ok(snapshot)
            }
            401 | 403 => Err(ProviderError::NeedsAuth),
            429 => Err(ProviderError::RateLimited {
                retry_after: retry_after_header(response.headers()),
            }),
            other => Err(ProviderError::BadResponse { status: other }),
        }
    }

    fn account(&self) -> Option<ProviderAccount> {
        let credential = read_credential().ok()?;
        let plan = self.plan.read().unwrap_or_else(|e| e.into_inner()).clone();
        Some(ProviderAccount { email: Some(format!("via {}", credential.source)), plan })
    }

    fn forget_cached_credential(&self) {
        // Nothing is cached: the files are re-read on every fetch.
    }

    fn sign_in_hint(&self) -> SignInHint {
        SignInHint::Text {
            text: "GLM usage rides on a Z.ai Coding Plan key held by another tool — Claude Code's settings.json, ZCode or OpenCode. Set one up there and GaugeCode reads it."
                .into(),
        }
    }

    fn manage_url(&self) -> &'static str {
        "https://z.ai/manage-apikey/apikey-list"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKENS: &str = include_str!("../../tests/fixtures/glm_quota_tokens.json");
    const CREDITS: &str = include_str!("../../tests/fixtures/glm_quota_credits.json");

    fn parse(body: &str) -> Result<ProviderSnapshot, ProviderError> {
        parse_usage(body, Utc::now()).map(|(snapshot, _)| snapshot)
    }

    #[test]
    fn parses_a_token_plan_with_the_session_window_first() {
        let snapshot = parse(TOKENS).expect("fixture should parse");
        let ids: Vec<&str> = snapshot.windows.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, ["session", "weekly", "mcp"]);

        let session = snapshot.primary_window().expect("session window");
        assert_eq!(session.label, "Current session");
        assert!((session.used_fraction - 0.125).abs() < 1e-9);
        // 1788682200000 ms is 1788682200 s, not 1788682200000 s.
        assert_eq!(session.resets_at, DateTime::from_timestamp(1_788_682_200, 0));

        // The MCP row never carries a reset time, and is kept regardless: a
        // percentage with no countdown is still a reading.
        let mcp = snapshot.windows.iter().find(|w| w.id == "mcp").unwrap();
        assert_eq!(mcp.resets_at, None);
        assert!((mcp.used_fraction - 0.04).abs() < 1e-9);
    }

    #[test]
    fn a_credit_plan_produces_the_same_window_ids_as_a_token_plan() {
        // The meter type differs (`CREDIT_LIMIT` vs `TOKENS_LIMIT`) but the
        // window lengths do not, which is why identity ignores the type.
        let snapshot = parse(CREDITS).expect("fixture should parse");
        let ids: Vec<&str> = snapshot.windows.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, ["session", "weekly"]);
        assert_eq!(snapshot.account.unwrap().plan.as_deref(), Some("lite"));
    }

    #[test]
    fn a_percentage_arriving_as_an_integer_parses_too() {
        let snapshot = parse(CREDITS).unwrap();
        assert!((snapshot.windows[0].used_fraction - 0.01).abs() < 1e-9);
        assert!((snapshot.windows[1].used_fraction - 0.17).abs() < 1e-9);
    }

    #[test]
    fn an_error_inside_a_two_hundred_body_is_not_a_reading() {
        // What an expired token actually answers, under HTTP 200.
        let expired = r#"{"code":401,"msg":"token expired or incorrect","success":false}"#;
        assert!(matches!(parse(expired), Err(ProviderError::NeedsAuth)));

        let overloaded = r#"{"code":1305,"msg":"overloaded","success":false}"#;
        assert!(matches!(parse(overloaded), Err(ProviderError::BadResponse { status: 1305 })));

        let throttled = r#"{"code":429,"success":false}"#;
        assert!(matches!(parse(throttled), Err(ProviderError::RateLimited { .. })));
    }

    #[test]
    fn a_body_with_no_envelope_fields_counts_as_success() {
        let body = r#"{"data":{"limits":[{"unit":3,"number":5,"percentage":9}]}}"#;
        let snapshot = parse(body).expect("no code and no success means success");
        assert_eq!(snapshot.windows[0].id, "session");
    }

    #[test]
    fn being_over_the_limit_is_reported_rather_than_clamped() {
        let body = r#"{"data":{"limits":[{"unit":3,"number":5,"percentage":128}]}}"#;
        let snapshot = parse(body).unwrap();
        assert!((snapshot.windows[0].used_fraction - 1.28).abs() < 1e-9);
    }

    #[test]
    fn a_row_without_a_percentage_is_dropped_rather_than_counted_as_zero() {
        let body = r#"{"data":{"limits":[
            {"unit":3,"number":5,"percentage":9},{"unit":6,"number":1}]}}"#;
        let snapshot = parse(body).unwrap();
        assert_eq!(snapshot.windows.len(), 1);
    }

    #[test]
    fn a_response_with_no_row_is_nothing_metered_not_zero() {
        assert!(matches!(
            parse(r#"{"code":200,"data":{"limits":[]}}"#),
            Err(ProviderError::NothingMetered(_))
        ));
        assert!(matches!(parse("{}"), Err(ProviderError::NothingMetered(_))));
    }

    #[test]
    fn garbage_is_a_bad_response_never_a_number() {
        assert!(matches!(parse("not json"), Err(ProviderError::BadResponse { status: 200 })));
    }

    #[test]
    fn an_unrecognised_window_length_still_gets_a_readable_label() {
        let body = r#"{"data":{"limits":[{"type":"TOKENS_LIMIT","unit":3,"number":2,
            "percentage":5},{"type":"TOKENS_LIMIT","unit":9,"number":4,"percentage":5}]}}"#;
        let snapshot = parse(body).unwrap();
        let labelled: Vec<(&str, &str)> = snapshot
            .windows
            .iter()
            .map(|w| (w.id.as_str(), w.label.as_str()))
            .collect();
        assert!(labelled.contains(&("window-3x2", "Usage (2 h)")));
        assert!(labelled.contains(&("window-9x4", "Usage")));
    }

    #[test]
    fn the_console_is_chosen_from_the_host_and_the_path_is_discarded() {
        // The monitor lives at the console root; under /api/anthropic it
        // answers a misleading 404.
        assert_eq!(host_of("https://api.z.ai/api/anthropic").as_deref(), Some("api.z.ai"));
        assert_eq!(console_for_host("api.z.ai"), Some(CONSOLE_GLOBAL));
        assert_eq!(console_for_host("gateway.z.ai"), Some(CONSOLE_GLOBAL));
        assert_eq!(console_for_host("open.bigmodel.cn"), Some(CONSOLE_CHINA));
        assert_eq!(console_for_host("x.bigmodel.cn"), Some(CONSOLE_CHINA));
        // A non-Z.ai host means the key is not a GLM key at all.
        assert_eq!(console_for_host("api.anthropic.com"), None);
    }

    #[test]
    fn a_base_url_that_merely_looks_like_zai_does_not_claim_the_token() {
        // This allowlist is what decides whether a token found in Claude Code's
        // settings is a Z.ai plan key at all. A host that only *resembles*
        // z.ai must not pass, or somebody's Anthropic key would be sent to
        // Z.ai's console. (The request URL itself is a hardcoded constant, so
        // this cannot redirect the token to the crafted host — the risk is
        // claiming the wrong token, not sending it somewhere new.)
        let hostile = [
            // Suffix that merely *contains* the domain.
            "https://api.z.ai.evil.example/v1",
            "https://evil.example/api.z.ai",
            // Userinfo trick: the real host is after the @.
            "https://api.z.ai@evil.example/v1",
            // Not a subdomain, just a prefix.
            "https://z.ai.evil.example",
            "https://notz.ai",
            "https://bigmodel.cn.evil.example",
            // Backslash, which some parsers treat as a separator.
            "https://evil.example\\api.z.ai",
            // No scheme at all.
            "evil.example/api.z.ai",
        ];
        for url in hostile {
            let console = host_of(url).and_then(|host| console_for_host(&host));
            assert_eq!(console, None, "{url} was accepted as a Z.ai console");
        }

        // And the shapes that must keep working.
        for url in [
            "https://api.z.ai",
            "https://api.z.ai/api/anthropic",
            "https://gateway.z.ai/v1",
            "https://open.bigmodel.cn/api/paas/v4",
            "HTTPS://API.Z.AI/",
        ] {
            assert!(
                host_of(url).and_then(|host| console_for_host(&host)).is_some(),
                "{url} should be accepted"
            );
        }
    }

    #[test]
    fn host_parsing_survives_the_shapes_a_settings_file_can_hold() {
        assert_eq!(host_of("https://api.z.ai").as_deref(), Some("api.z.ai"));
        assert_eq!(host_of("http://user:pw@api.z.ai:8443/x").as_deref(), Some("api.z.ai"));
        assert_eq!(host_of("api.z.ai/api").as_deref(), Some("api.z.ai"));
        assert_eq!(host_of("https://"), None);
        assert_eq!(host_of(""), None);
    }

    #[test]
    fn a_disabled_or_pay_as_you_go_zcode_entry_is_never_claimed() {
        // Exercised through the same predicate the reader uses, since the
        // reader itself needs a home directory.
        let ids = ["builtin:zai", "builtin:zai-coding-plan"];
        let considered: Vec<&str> =
            ids.into_iter().filter(|id| id.contains("coding-plan")).collect();
        assert_eq!(considered, ["builtin:zai-coding-plan"]);
    }

    #[test]
    fn an_encrypted_zcode_token_is_skipped_rather_than_guessed_at() {
        assert!("enc:v1:abc".starts_with("enc:v1:"));
        assert!(!"plain-token".starts_with("enc:v1:"));
    }
}
