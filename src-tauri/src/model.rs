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
    Glm,
    Grok,
    OpenCode,
}

impl ProviderId {
    pub const ALL: [ProviderId; 6] = [
        ProviderId::Claude,
        ProviderId::Cursor,
        ProviderId::Codex,
        ProviderId::Glm,
        ProviderId::Grok,
        ProviderId::OpenCode,
    ];

    /// The three that are on by default. The rest are opt-in: most people use
    /// one or two of these tools, and four rows reading "needs sign-in" would
    /// look like a broken app rather than an available feature.
    pub const DEFAULT_ENABLED: [ProviderId; 3] =
        [ProviderId::Claude, ProviderId::Cursor, ProviderId::Codex];

    pub fn display_name(self) -> &'static str {
        match self {
            ProviderId::Claude => "Claude Code",
            ProviderId::Cursor => "Cursor",
            ProviderId::Codex => "Codex",
            ProviderId::Glm => "GLM",
            ProviderId::Grok => "Grok",
            ProviderId::OpenCode => "OpenCode",
        }
    }

    /// Process names that mean "this tool is open right now" (SPEC §8).
    pub fn process_names(self) -> &'static [&'static str] {
        match self {
            ProviderId::Claude => &["claude", "claude.exe"],
            ProviderId::Cursor => &["cursor", "cursor.exe"],
            ProviderId::Codex => &["codex", "codex.exe"],
            // GLM usage rides on a key another tool holds, so any of the tools
            // that can hold one counts as "in use".
            ProviderId::Glm => &["claude", "claude.exe", "zcode", "zcode.exe"],
            ProviderId::Grok => &["grok", "grok.exe"],
            ProviderId::OpenCode => &["opencode", "opencode.exe"],
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

/// Colour band of a used fraction (SPEC §9.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Band {
    Ok,
    Warn,
    Hot,
    /// No usable number: `needsAuth`, `disabled` or `error`.
    Off,
}

impl Band {
    pub fn of(used_fraction: f64) -> Band {
        if used_fraction < 0.5 {
            Band::Ok
        } else if used_fraction <= 0.8 {
            Band::Warn
        } else {
            Band::Hot
        }
    }

    /// Colour used to draw the tray icon on Windows. Must match `--color-band-*`
    /// in `src/styles.css`.
    pub fn rgb(self) -> [u8; 3] {
        match self {
            Band::Ok => [0x22, 0xc5, 0x5e],
            Band::Warn => [0xf5, 0x9e, 0x0b],
            Band::Hot => [0xef, 0x44, 0x44],
            Band::Off => [0x6b, 0x72, 0x80],
        }
    }
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

impl ProviderSnapshot {
    /// The window the main ring shows. `session` when the provider has one,
    /// otherwise the first window (Codex reports `primary`/`secondary`).
    pub fn primary_window(&self) -> Option<&LimitWindow> {
        self.windows
            .iter()
            .find(|w| w.id == SESSION_WINDOW_ID)
            .or_else(|| self.windows.first())
    }
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

impl ProviderStatus {
    /// A stale or errored provider is drawn dimmed (SPEC §9.1).
    pub fn is_dimmed(&self) -> bool {
        !matches!(self, ProviderStatus::Fresh)
    }
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

    #[test]
    fn bands_follow_the_spec_thresholds() {
        assert_eq!(Band::of(0.0), Band::Ok);
        assert_eq!(Band::of(0.499), Band::Ok);
        assert_eq!(Band::of(0.5), Band::Warn);
        assert_eq!(Band::of(0.8), Band::Warn);
        assert_eq!(Band::of(0.801), Band::Hot);
        assert_eq!(Band::of(1.0), Band::Hot);
    }

    #[test]
    fn primary_window_prefers_session_then_falls_back_to_first() {
        let mut snapshot = ProviderSnapshot {
            provider: ProviderId::Claude,
            fidelity: Fidelity::Official,
            windows: vec![window("weekly_all"), window("session")],
            fetched_at: Utc::now(),
            account: None,
        };
        assert_eq!(snapshot.primary_window().unwrap().id, "session");

        snapshot.windows = vec![window("primary"), window("secondary")];
        assert_eq!(snapshot.primary_window().unwrap().id, "primary");

        snapshot.windows.clear();
        assert!(snapshot.primary_window().is_none());
    }
}
