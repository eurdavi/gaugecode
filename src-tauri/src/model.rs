//! Shared data model (SPEC §5).
//!
//! Mirrored by hand in `src/types/usage.ts`. If you change one, change the
//! other in the same commit.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProviderId {
    Claude,
    Cursor,
    Codex,
}

impl ProviderId {
    pub const ALL: [ProviderId; 3] = [ProviderId::Claude, ProviderId::Cursor, ProviderId::Codex];

    pub fn display_name(self) -> &'static str {
        match self {
            ProviderId::Claude => "Claude Code",
            ProviderId::Cursor => "Cursor",
            ProviderId::Codex => "Codex",
        }
    }
}

/// How trustworthy a snapshot is.
///
/// * `Official` — numbers came straight from the vendor's usage endpoint.
/// * `Derived`  — estimated locally (e.g. from transcripts). Backlog; never silent.
/// * `Manual`   — entered by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Fidelity {
    Official,
    Derived,
    Manual,
}

/// One rate-limit window as reported by the provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LimitWindow {
    /// `"session"`, `"weekly_all"`, `"weekly_opus"`, `"primary"`, ...
    pub id: String,
    pub label: String,
    /// `0.0..=1.0`. Always taken from the response — never estimated here.
    pub used_fraction: f64,
    pub resets_at: Option<DateTime<Utc>>,
}

/// Window id that every provider's main ring shows (SPEC §5).
pub const SESSION_WINDOW_ID: &str = "session";

/// Stable sort: the `session` window first, everything else in provider order.
pub fn sort_windows(windows: &mut [LimitWindow]) {
    windows.sort_by_key(|w| if w.id == SESSION_WINDOW_ID { 0 } else { 1 });
}

/// Display-only account info, read from the credential without any network call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderAccount {
    pub email: Option<String>,
    pub plan: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderSnapshot {
    pub provider: ProviderId,
    pub fidelity: Fidelity,
    /// Ordered by [`sort_windows`].
    pub windows: Vec<LimitWindow>,
    pub fetched_at: DateTime<Utc>,
    pub account: Option<ProviderAccount>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("no credential found")]
    NeedsAuth,
    #[error("credential expired")]
    CredentialExpired,
    #[error("access denied by provider")]
    AccessDenied,
    #[error("rate limited (retry after {retry_after:?})")]
    RateLimited { retry_after: Duration },
    #[error("unexpected response (http {status})")]
    BadResponse { status: u16 },
    #[error("nothing metered: {0}")]
    NothingMetered(String),
    #[error("io: {0}")]
    Io(String),
}

/// What the UI shows for a provider. Emitted on `usage:status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProviderStatus {
    Fresh,
    /// Last good snapshot is being shown; `age_secs` since it was fetched.
    Stale { age_secs: u64 },
    NeedsAuth,
    Error { message: String },
    Disabled,
}

/// Where the user should go to authenticate. The app never logs in by itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SignInHint {
    OpenUrl { url: String },
    OpenApp { app: String },
    Text { text: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(id: &str) -> LimitWindow {
        LimitWindow {
            id: id.to_string(),
            label: id.to_string(),
            used_fraction: 0.5,
            resets_at: None,
        }
    }

    #[test]
    fn session_window_sorts_first_and_rest_keeps_order() {
        let mut windows = vec![window("weekly_opus"), window("weekly_all"), window("session")];
        sort_windows(&mut windows);
        let ids: Vec<&str> = windows.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, ["session", "weekly_opus", "weekly_all"]);
    }

    #[test]
    fn provider_id_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&ProviderId::Claude).unwrap(), "\"claude\"");
    }

    #[test]
    fn status_serializes_with_kind_tag() {
        let json = serde_json::to_value(ProviderStatus::Stale { age_secs: 90 }).unwrap();
        assert_eq!(json, serde_json::json!({ "kind": "stale", "age_secs": 90 }));
        let json = serde_json::to_value(ProviderStatus::NeedsAuth).unwrap();
        assert_eq!(json, serde_json::json!({ "kind": "needs_auth" }));
    }

    #[test]
    fn snapshot_round_trips_through_json() {
        let snapshot = ProviderSnapshot {
            provider: ProviderId::Codex,
            fidelity: Fidelity::Official,
            windows: vec![window("primary")],
            fetched_at: Utc::now(),
            account: Some(ProviderAccount { email: None, plan: Some("plus".into()) }),
        };
        let json = serde_json::to_string(&snapshot).unwrap();
        let back: ProviderSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(back, snapshot);
    }
}
