//! Grok adapter (xAI) — `Fidelity::Official`.
//!
//! Reads `~/.grok/auth.json`, which the Grok CLI writes on `grok login`, and
//! asks xAI's own billing endpoint.
//!
//! One thing here is a security property rather than a detail: Grok also
//! supports a customer identity provider, whose token is meant for that
//! customer's private proxy. Sending it to the public endpoint would hand
//! someone else's credential to xAI, so only entries issued by `auth.x.ai` are
//! ever used — see [`is_trusted`].

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::model::{
    sort_windows, Fidelity, LimitWindow, ProviderAccount, ProviderError, ProviderId,
    ProviderSnapshot, SignInHint,
};
use crate::providers::UsageProvider;

const BILLING_URL: &str = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";
/// A client identifier, not the token. Easy to misread as a second credential.
const CLIENT_HEADER: &str = "X-XAI-Token-Auth";
const CLIENT_VALUE: &str = "xai-grok-cli";
const TIMEOUT: Duration = Duration::from_secs(15);
const MANAGE_URL: &str = "https://grok.com";
/// The only issuer whose token belongs to the public endpoint.
const TRUSTED_ISSUER: &str = "https://auth.x.ai";
/// Applied when an entry carries no expiry, so a usable token is not discarded
/// for failing to say when it dies.
const ASSUMED_LIFETIME_DAYS: i64 = 30;

// ---------------------------------------------------------------------------
// Credential
// ---------------------------------------------------------------------------

/// One signed-in entry. The token field is `key`, not `access_token`.
#[derive(Debug, Clone, Deserialize)]
struct AuthEntry {
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    oidc_issuer: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Credential {
    token: String,
    email: Option<String>,
    expires_at: Option<DateTime<Utc>>,
}

impl Credential {
    fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at.is_some_and(|at| at <= now)
    }
}

/// An entry belongs to the public endpoint if its map key or its `oidc_issuer`
/// names xAI's own issuer. Anything else is a customer IdP token.
fn is_trusted(map_key: &str, entry: &AuthEntry) -> bool {
    map_key.starts_with(TRUSTED_ISSUER)
        || entry.oidc_issuer.as_deref() == Some(TRUSTED_ISSUER)
}

/// Picks a credential out of the file.
///
/// `BTreeMap` rather than a hash map on purpose: with two signed-in entries the
/// answer has to be the same on every launch, not whatever the map iterated
/// first. Live entries win; if none is live the first trusted entry is returned
/// so the caller can report "expired" instead of "signed out".
fn choose(entries: &BTreeMap<String, AuthEntry>, now: DateTime<Utc>) -> Option<Credential> {
    let candidates: Vec<Credential> = entries
        .iter()
        .filter(|(map_key, entry)| is_trusted(map_key, entry))
        .filter_map(|(_, entry)| {
            let token = entry.key.as_deref()?.trim();
            if token.is_empty() {
                return None;
            }
            Some(Credential {
                token: token.to_string(),
                email: entry.email.clone(),
                // A missing expiry counts as live, with an assumed lifetime, so
                // it is not mistaken for a dead token.
                expires_at: Some(
                    entry.expires_at.unwrap_or(now + chrono::Duration::days(ASSUMED_LIFETIME_DAYS)),
                ),
            })
        })
        .collect();

    candidates
        .iter()
        .find(|credential| !credential.is_expired(now))
        .or_else(|| candidates.first())
        .cloned()
}

fn auth_path() -> Result<PathBuf, ProviderError> {
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
    let Some(home) = home else {
        return Err(ProviderError::Io("neither USERPROFILE nor HOME is set".into()));
    };
    Ok(PathBuf::from(home).join(".grok").join("auth.json"))
}

fn read_credential() -> Result<Credential, ProviderError> {
    let raw = match std::fs::read_to_string(auth_path()?) {
        Ok(raw) => raw,
        // The CLI is not installed, or was never signed in.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Err(ProviderError::NeedsAuth)
        }
        Err(error) => return Err(ProviderError::Io(error.to_string())),
    };
    parse_credential(&raw, Utc::now())
}

fn parse_credential(raw: &str, now: DateTime<Utc>) -> Result<Credential, ProviderError> {
    let entries: BTreeMap<String, AuthEntry> = serde_json::from_str(raw)
        .map_err(|_| ProviderError::Io("auth.json is not the expected shape".into()))?;
    choose(&entries, now).ok_or(ProviderError::NeedsAuth)
}

// ---------------------------------------------------------------------------
// Response
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct BillingResponse {
    #[serde(default)]
    config: Option<BillingConfig>,
}

#[derive(Debug, Deserialize)]
struct BillingConfig {
    #[serde(default, rename = "currentPeriod")]
    current_period: Option<Period>,
    /// 0–100. When present it is the whole answer.
    #[serde(default, rename = "creditUsagePercent")]
    credit_usage_percent: Option<f64>,
    #[serde(default, rename = "productUsage")]
    product_usage: Vec<ProductUsage>,
    #[serde(default, rename = "billingPeriodEnd")]
    billing_period_end: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct Period {
    #[serde(default)]
    end: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
struct ProductUsage {
    #[serde(default)]
    product: Option<String>,
    #[serde(default, rename = "usagePercent")]
    usage_percent: Option<f64>,
}

/// The wire name is one word; the usage screen writes two. `GrokBuild` reads as
/// `Grok Build`.
fn humanize(product: &str) -> String {
    let mut out = String::with_capacity(product.len() + 2);
    for (index, character) in product.chars().enumerate() {
        if index > 0 && character.is_uppercase() {
            out.push(' ');
        }
        out.push(character);
    }
    out
}

fn fraction(percent: f64) -> f64 {
    (percent / 100.0).clamp(0.0, 1.0)
}

/// The headline window is always `credits`, so the first window emitted takes
/// that id whichever branch produced it — otherwise the tooltip would show a
/// perfectly good bar while the ring showed a dash.
fn parse_usage(
    body: &str,
    fetched_at: DateTime<Utc>,
    email: Option<String>,
) -> Result<ProviderSnapshot, ProviderError> {
    let response: BillingResponse =
        serde_json::from_str(body).map_err(|_| ProviderError::BadResponse { status: 200 })?;
    let Some(config) = response.config else {
        return Err(ProviderError::BadResponse { status: 200 });
    };

    let resets_at = config
        .current_period
        .and_then(|period| period.end)
        .or(config.billing_period_end);

    let mut windows = Vec::new();
    match config.credit_usage_percent {
        Some(percent) => windows.push(LimitWindow {
            id: "credits".into(),
            label: config
                .product_usage
                .first()
                .and_then(|usage| usage.product.as_deref())
                .map(humanize)
                .unwrap_or_else(|| "Grok Build".to_string()),
            used_fraction: fraction(percent),
            resets_at,
        }),
        None => {
            for usage in &config.product_usage {
                let Some(percent) = usage.usage_percent else { continue };
                let product = usage.product.as_deref().unwrap_or_default();
                windows.push(LimitWindow {
                    id: if windows.is_empty() {
                        "credits".to_string()
                    } else {
                        product.to_string()
                    },
                    label: if product.is_empty() {
                        "Usage".to_string()
                    } else {
                        humanize(product)
                    },
                    used_fraction: fraction(percent),
                    resets_at,
                });
            }
        }
    }

    if windows.is_empty() {
        return Err(ProviderError::NothingMetered(
            "Grok has nothing metered on this account yet".into(),
        ));
    }

    sort_windows(&mut windows);
    Ok(ProviderSnapshot {
        provider: ProviderId::Grok,
        fidelity: Fidelity::Official,
        windows,
        fetched_at,
        account: Some(ProviderAccount { email, plan: None }),
    })
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

pub struct GrokProvider {
    client: reqwest::Client,
}

impl GrokProvider {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .user_agent(concat!("GaugeCode/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client }
    }
}

impl Default for GrokProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl UsageProvider for GrokProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Grok
    }

    fn fidelity(&self) -> Fidelity {
        Fidelity::Official
    }

    async fn fetch_snapshot(&self) -> Result<ProviderSnapshot, ProviderError> {
        let credential = read_credential()?;
        // Refreshing the token is the CLI's job; writing the file ourselves
        // would race it. So an expired credential is reported, not renewed.
        if credential.is_expired(Utc::now()) {
            return Err(ProviderError::CredentialExpired);
        }

        let started = Instant::now();
        let response = self
            .client
            .get(BILLING_URL)
            .bearer_auth(&credential.token)
            .header(CLIENT_HEADER, CLIENT_VALUE)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|error| ProviderError::Io(error.without_url().to_string()))?;

        let status = response.status();
        // Status, provider and duration only. The body is billing data, so it
        // does not go to a log either.
        tracing::debug!(
            provider = "grok",
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
                parse_usage(&body, Utc::now(), credential.email.clone())
            }
            401 | 403 => Err(ProviderError::NeedsAuth),
            // This endpoint sends no `Retry-After`, so the scheduler's own
            // schedule decides the wait.
            429 => Err(ProviderError::RateLimited { retry_after: Duration::ZERO }),
            other => Err(ProviderError::BadResponse { status: other }),
        }
    }

    fn account(&self) -> Option<ProviderAccount> {
        let credential = read_credential().ok()?;
        Some(ProviderAccount { email: credential.email, plan: None })
    }

    fn forget_cached_credential(&self) {
        // Nothing is cached: the file is re-read on every fetch.
    }

    fn sign_in_hint(&self) -> SignInHint {
        SignInHint::Text {
            text: "Run `grok login` in a terminal; GaugeCode reads the token it leaves behind."
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

    const OK: &str = include_str!("../../tests/fixtures/grok_billing_ok.json");
    const AUTH: &str = include_str!("../../tests/fixtures/grok_auth.json");

    fn at(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text).unwrap().with_timezone(&Utc)
    }

    fn parse(body: &str) -> Result<ProviderSnapshot, ProviderError> {
        parse_usage(body, Utc::now(), None)
    }

    #[test]
    fn parses_the_credits_shape() {
        let snapshot = parse(OK).expect("fixture should parse");
        assert_eq!(snapshot.provider, ProviderId::Grok);
        assert_eq!(snapshot.windows.len(), 1);

        let window = snapshot.primary_window().unwrap();
        assert_eq!(window.id, "credits");
        // The wire name is one word; the label is two.
        assert_eq!(window.label, "Grok Build");
        assert!((window.used_fraction - 0.08).abs() < 1e-9);
        assert_eq!(window.resets_at, Some(at("2026-09-12T08:21:18.802818+00:00")));
    }

    #[test]
    fn the_period_end_wins_over_the_billing_period_end() {
        let body = r#"{"config":{"creditUsagePercent":5,
            "currentPeriod":{"end":"2026-09-12T00:00:00Z"},
            "billingPeriodEnd":"2026-10-01T00:00:00Z"}}"#;
        let snapshot = parse(body).unwrap();
        assert_eq!(snapshot.windows[0].resets_at, Some(at("2026-09-12T00:00:00Z")));
    }

    #[test]
    fn the_billing_period_end_is_the_fallback() {
        let body =
            r#"{"config":{"creditUsagePercent":5,"billingPeriodEnd":"2026-10-01T00:00:00Z"}}"#;
        let snapshot = parse(body).unwrap();
        assert_eq!(snapshot.windows[0].resets_at, Some(at("2026-10-01T00:00:00Z")));
    }

    #[test]
    fn without_a_credit_percent_the_products_become_the_windows() {
        let body = r#"{"config":{"productUsage":[
            {"product":"GrokBuild","usagePercent":12},
            {"product":"GrokChat","usagePercent":40}]}}"#;
        let snapshot = parse(body).unwrap();
        // The first window must be `credits`, or the ring finds nothing to show.
        let ids: Vec<&str> = snapshot.windows.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, ["credits", "GrokChat"]);
        assert_eq!(snapshot.windows[0].label, "Grok Build");
        assert_eq!(snapshot.windows[1].label, "Grok Chat");
    }

    #[test]
    fn a_product_without_a_percent_is_skipped_rather_than_counted_as_zero() {
        let body = r#"{"config":{"productUsage":[
            {"product":"GrokBuild"},{"product":"GrokChat","usagePercent":40}]}}"#;
        let snapshot = parse(body).unwrap();
        assert_eq!(snapshot.windows.len(), 1);
        assert_eq!(snapshot.windows[0].label, "Grok Chat");
    }

    #[test]
    fn an_account_with_nothing_metered_is_not_an_error() {
        assert!(matches!(
            parse(r#"{"config":{}}"#),
            Err(ProviderError::NothingMetered(_))
        ));
    }

    #[test]
    fn a_body_without_config_is_a_bad_response() {
        assert!(matches!(parse("{}"), Err(ProviderError::BadResponse { status: 200 })));
        assert!(matches!(parse("not json"), Err(ProviderError::BadResponse { status: 200 })));
    }

    #[test]
    fn wire_names_are_humanized_without_touching_the_first_letter() {
        assert_eq!(humanize("GrokBuild"), "Grok Build");
        assert_eq!(humanize("Grok"), "Grok");
        assert_eq!(humanize("GrokDeepSearch"), "Grok Deep Search");
        assert_eq!(humanize(""), "");
    }

    #[test]
    fn the_fixture_credential_parses_with_its_address() {
        let credential = parse_credential(AUTH, at("2026-09-08T00:00:00Z")).unwrap();
        assert_eq!(credential.email.as_deref(), Some("user@example.com"));
        assert!(!credential.is_expired(at("2026-09-08T00:00:00Z")));
    }

    #[test]
    fn a_token_from_another_identity_provider_is_never_sent_to_xai() {
        // The security property: this token belongs to a customer's private
        // proxy, and sending it to the public endpoint would hand it to xAI.
        let raw = r#"{"https://sso.acme.example::grok-cli":{
            "key":"private-proxy-token","oidc_issuer":"https://sso.acme.example"}}"#;
        assert!(matches!(
            parse_credential(raw, Utc::now()),
            Err(ProviderError::NeedsAuth)
        ));
    }

    #[test]
    fn an_entry_is_trusted_by_its_map_key_or_by_its_issuer_field() {
        let bare = AuthEntry { key: None, expires_at: None, email: None, oidc_issuer: None };
        assert!(is_trusted("https://auth.x.ai::grok-cli", &bare));
        assert!(!is_trusted("https://sso.acme.example::grok-cli", &bare));

        let with_issuer = AuthEntry {
            oidc_issuer: Some(TRUSTED_ISSUER.to_string()),
            ..AuthEntry { key: None, expires_at: None, email: None, oidc_issuer: None }
        };
        assert!(is_trusted("anything", &with_issuer));
    }

    #[test]
    fn a_live_entry_wins_over_an_expired_one_whatever_the_order() {
        let raw = r#"{
            "https://auth.x.ai::a":{"key":"dead","expires_at":"2020-01-01T00:00:00Z"},
            "https://auth.x.ai::b":{"key":"alive","expires_at":"2030-01-01T00:00:00Z"}}"#;
        let credential = parse_credential(raw, at("2026-09-08T00:00:00Z")).unwrap();
        assert_eq!(credential.token, "alive");
    }

    #[test]
    fn all_entries_expired_reports_expired_rather_than_signed_out() {
        let raw = r#"{"https://auth.x.ai::a":{"key":"dead",
            "expires_at":"2020-01-01T00:00:00Z"}}"#;
        let credential = parse_credential(raw, at("2026-09-08T00:00:00Z")).unwrap();
        assert!(credential.is_expired(at("2026-09-08T00:00:00Z")));
    }

    #[test]
    fn a_missing_expiry_counts_as_live_rather_than_dead() {
        let raw = r#"{"https://auth.x.ai::a":{"key":"no-expiry"}}"#;
        let now = at("2026-09-08T00:00:00Z");
        let credential = parse_credential(raw, now).unwrap();
        assert!(!credential.is_expired(now));
    }

    #[test]
    fn an_empty_token_counts_as_signed_out() {
        let raw = r#"{"https://auth.x.ai::a":{"key":"  "}}"#;
        assert!(matches!(parse_credential(raw, Utc::now()), Err(ProviderError::NeedsAuth)));
    }

    #[test]
    fn a_malformed_file_is_an_io_error_without_echoing_it() {
        let error = parse_credential(r#"{"a":{"key":42,"note":"secret-value"}}"#, Utc::now())
            .unwrap_err();
        assert!(matches!(error, ProviderError::Io(_)));
        assert!(!error.to_string().contains("secret-value"));
    }
}
