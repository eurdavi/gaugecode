//! Cursor adapter (SPEC §7.2) — `Fidelity::Official`.
//!
//! Cursor keeps its session token in the same SQLite database the editor uses
//! for all its global state. We open that database **read-only** and never write
//! to it — see [`read_credential`] for how a database the editor has locked is
//! handled.
//!
//! Like every adapter here, the endpoint is undocumented: a parse failure is a
//! [`ProviderError::BadResponse`], never an invented number.

use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;

use crate::model::{
    sort_windows, Fidelity, LimitWindow, ProviderAccount, ProviderError, ProviderId,
    ProviderSnapshot, SignInHint,
};
use crate::providers::{retry_after_header, UsageProvider};

const USAGE_URL: &str = "https://cursor.com/api/usage-summary";
const TIMEOUT: Duration = Duration::from_secs(15);
const MANAGE_URL: &str = "https://cursor.com/dashboard";

const KEY_ACCESS_TOKEN: &str = "cursorAuth/accessToken";
const KEY_ACCOUNT_ID: &str = "cursorAuth/stripeMembershipAuthId";
const KEY_EMAIL: &str = "cursorAuth/cachedEmail";
const KEY_MEMBERSHIP: &str = "cursorAuth/stripeMembershipType";

// ---------------------------------------------------------------------------
// Credential
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct Credential {
    access_token: String,
    account_id: String,
    email: Option<String>,
    membership: Option<String>,
}

impl Credential {
    /// The exact cookie Cursor's own web client sends. Never logged.
    fn cookie(&self) -> String {
        format!("WorkosCursorSessionToken={}::{}", self.account_id, self.access_token)
    }
}

#[cfg(target_os = "windows")]
fn state_db_path() -> Result<PathBuf, ProviderError> {
    let Some(appdata) = std::env::var_os("APPDATA") else {
        return Err(ProviderError::Io("APPDATA is not set".into()));
    };
    Ok(PathBuf::from(appdata)
        .join("Cursor")
        .join("User")
        .join("globalStorage")
        .join("state.vscdb"))
}

#[cfg(target_os = "macos")]
fn state_db_path() -> Result<PathBuf, ProviderError> {
    let Some(home) = std::env::var_os("HOME") else {
        return Err(ProviderError::Io("HOME is not set".into()));
    };
    Ok(PathBuf::from(home)
        .join("Library")
        .join("Application Support")
        .join("Cursor")
        .join("User")
        .join("globalStorage")
        .join("state.vscdb"))
}

/// Linux follows the XDG base directory spec, which Cursor honours.
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn state_db_path() -> Result<PathBuf, ProviderError> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    let Some(config) = config else {
        return Err(ProviderError::Io("neither XDG_CONFIG_HOME nor HOME is set".into()));
    };
    Ok(config.join("Cursor").join("User").join("globalStorage").join("state.vscdb"))
}

/// Reads the four keys we care about from `ItemTable`.
///
/// `SQLITE_OPEN_READ_ONLY` is not a nicety — writing to the editor's live
/// database could corrupt the user's Cursor install (AGENTS.md).
fn read_item_table(path: &Path) -> rusqlite::Result<Vec<(String, String)>> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut statement = connection
        .prepare("SELECT key, value FROM ItemTable WHERE key IN (?1, ?2, ?3, ?4)")?;
    let rows = statement.query_map(
        [KEY_ACCESS_TOKEN, KEY_ACCOUNT_ID, KEY_EMAIL, KEY_MEMBERSHIP],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    )?;
    rows.collect()
}

/// A scratch directory that deletes itself, so a copy of somebody's credential
/// database cannot outlive the read that needed it — not on an early return,
/// not on a panic.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Creates a private directory to copy the database into.
///
/// Two things here are deliberate, because on Linux and macOS the system temp
/// directory is shared with every other local user:
///
/// * the name carries a high-resolution timestamp, so it cannot be guessed
///   ahead of time the way a bare process id could;
/// * `create_dir`, **not** `create_dir_all` — an existing path means somebody
///   got there first, possibly with a symlink pointing somewhere they can read,
///   and reusing it would hand them the credential database. Failing is the
///   only safe answer.
fn scratch() -> Result<Scratch, ProviderError> {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_nanos())
        .unwrap_or_default();
    let dir = std::env::temp_dir()
        .join(format!("gaugecode-cursor-{}-{unique:x}", std::process::id()));

    std::fs::create_dir(&dir).map_err(|error| ProviderError::Io(error.to_string()))?;

    // Keep other users out of the directory itself, not just the files in it.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    }

    Ok(Scratch(dir))
}

/// Copies the database aside so it can be read while Cursor holds a lock on it.
/// The write-ahead log has to come along or the copy can be missing the most
/// recent transaction — including a token that was just refreshed.
fn read_item_table_from_copy(path: &Path) -> Result<Vec<(String, String)>, ProviderError> {
    let scratch = scratch()?;
    let copy = scratch.0.join("state.vscdb");

    std::fs::copy(path, &copy).map_err(|error| ProviderError::Io(error.to_string()))?;
    for suffix in ["-wal", "-shm"] {
        let sidecar = PathBuf::from(format!("{}{suffix}", path.display()));
        if sidecar.exists() {
            let destination = PathBuf::from(format!("{}{suffix}", copy.display()));
            let _ = std::fs::copy(&sidecar, &destination);
        }
    }

    read_item_table(&copy).map_err(|error| ProviderError::Io(error.to_string()))
}

/// Turns the raw rows into a credential. Values are quoted JSON strings in some
/// Cursor versions and bare strings in others, so both are accepted.
fn credential_from_rows(rows: &[(String, String)]) -> Result<Credential, ProviderError> {
    let value = |wanted: &str| -> Option<String> {
        rows.iter()
            .find(|(key, _)| key == wanted)
            .map(|(_, value)| unquote(value))
            .filter(|value| !value.is_empty())
    };

    // No token means Cursor is not signed in here. That is `NeedsAuth`, not an
    // error the user has to debug.
    let (Some(access_token), Some(account_id)) = (value(KEY_ACCESS_TOKEN), value(KEY_ACCOUNT_ID))
    else {
        return Err(ProviderError::NeedsAuth);
    };

    Ok(Credential {
        access_token,
        account_id,
        email: value(KEY_EMAIL),
        membership: value(KEY_MEMBERSHIP),
    })
}

fn unquote(value: &str) -> String {
    let trimmed = value.trim();
    serde_json::from_str::<String>(trimmed).unwrap_or_else(|_| trimmed.to_string())
}

fn read_credential() -> Result<Credential, ProviderError> {
    let path = state_db_path()?;
    if !path.exists() {
        // Cursor was never installed for this user, or never signed in.
        return Err(ProviderError::NeedsAuth);
    }

    let rows = match read_item_table(&path) {
        Ok(rows) => rows,
        Err(error) => {
            tracing::debug!(%error, "cursor state db is busy; reading a copy instead");
            read_item_table_from_copy(&path)?
        }
    };
    credential_from_rows(&rows)
}

// ---------------------------------------------------------------------------
// Response
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct UsageResponse {
    #[serde(default, rename = "individualUsage")]
    individual_usage: Option<IndividualUsage>,
    #[serde(default, rename = "billingCycleEnd")]
    billing_cycle_end: Option<DateTime<Utc>>,
    #[serde(default, rename = "membershipType")]
    membership_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IndividualUsage {
    #[serde(default)]
    plan: Option<PlanUsage>,
    #[serde(default, rename = "onDemand")]
    on_demand: Option<OnDemandUsage>,
}

#[derive(Debug, Deserialize)]
struct PlanUsage {
    /// 0–100. Absent means the window is absent, not zero.
    #[serde(default, rename = "totalPercentUsed")]
    total_percent_used: Option<f64>,
    #[serde(default, rename = "apiPercentUsed")]
    api_percent_used: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct OnDemandUsage {
    #[serde(default)]
    used: Option<f64>,
    #[serde(default)]
    limit: Option<f64>,
}

fn fraction(percent: f64) -> f64 {
    (percent / 100.0).clamp(0.0, 1.0)
}

/// Cursor meters a billing cycle rather than a rolling session, so the primary
/// window is the plan itself. It is emitted first, which is what
/// `ProviderSnapshot::primary_window` falls back to when there is no `session`.
fn parse_usage(
    body: &str,
    fetched_at: DateTime<Utc>,
    credential_email: Option<String>,
    credential_plan: Option<String>,
) -> Result<ProviderSnapshot, ProviderError> {
    let response: UsageResponse =
        serde_json::from_str(body).map_err(|_| ProviderError::BadResponse { status: 200 })?;

    let resets_at = response.billing_cycle_end;
    let mut windows = Vec::new();

    if let Some(usage) = response.individual_usage.as_ref() {
        if let Some(plan) = usage.plan.as_ref() {
            if let Some(percent) = plan.total_percent_used {
                windows.push(LimitWindow {
                    id: "plan".into(),
                    label: "Plan (billing cycle)".into(),
                    used_fraction: fraction(percent),
                    resets_at,
                });
            }
            if let Some(percent) = plan.api_percent_used {
                windows.push(LimitWindow {
                    id: "api".into(),
                    label: "Included API usage".into(),
                    used_fraction: fraction(percent),
                    resets_at,
                });
            }
        }
        // A zero limit would make the fraction meaningless, so the window is
        // dropped rather than reported as 0 % or 100 %.
        if let Some(on_demand) = usage.on_demand.as_ref() {
            if let (Some(used), Some(limit)) = (on_demand.used, on_demand.limit) {
                if limit > 0.0 {
                    windows.push(LimitWindow {
                        id: "on_demand".into(),
                        label: "On-demand spend".into(),
                        used_fraction: (used / limit).clamp(0.0, 1.0),
                        resets_at,
                    });
                }
            }
        }
    }

    if windows.is_empty() {
        return Err(ProviderError::NothingMetered(
            "usage summary carried no plan window".into(),
        ));
    }

    sort_windows(&mut windows);
    Ok(ProviderSnapshot {
        provider: ProviderId::Cursor,
        fidelity: Fidelity::Official,
        windows,
        fetched_at,
        account: Some(ProviderAccount {
            email: credential_email,
            plan: response.membership_type.or(credential_plan),
        }),
    })
}

// ---------------------------------------------------------------------------
// Provider
// ---------------------------------------------------------------------------

pub struct CursorProvider {
    client: reqwest::Client,
    /// Process lifetime only; never persisted (SPEC §11).
    cached: RwLock<Option<Credential>>,
}

impl CursorProvider {
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

    async fn request(&self, credential: &Credential) -> Result<ProviderSnapshot, ProviderError> {
        let started = Instant::now();
        let response = self
            .client
            .get(USAGE_URL)
            .header(reqwest::header::COOKIE, credential.cookie())
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(|error| ProviderError::Io(error.without_url().to_string()))?;

        let status = response.status();
        // Status, provider and duration only — never the cookie.
        tracing::debug!(
            provider = "cursor",
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
                parse_usage(
                    &body,
                    Utc::now(),
                    credential.email.clone(),
                    credential.membership.clone(),
                )
            }
            401 | 403 => Err(ProviderError::AccessDenied),
            429 => Err(ProviderError::RateLimited { retry_after: retry_after_header(response.headers()) }),
            other => Err(ProviderError::BadResponse { status: other }),
        }
    }
}

impl Default for CursorProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl UsageProvider for CursorProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Cursor
    }

    fn fidelity(&self) -> Fidelity {
        Fidelity::Official
    }

    async fn fetch_snapshot(&self) -> Result<ProviderSnapshot, ProviderError> {
        let credential = self.credential(false)?;
        match self.request(&credential).await {
            Err(ProviderError::AccessDenied) => {
                // Cursor rotates the token in the same database row, so a 401 is
                // usually just our cached copy going stale. Re-read once.
                self.forget_cached_credential();
                let credential = self.credential(true)?;
                match self.request(&credential).await {
                    Err(ProviderError::AccessDenied) => Err(ProviderError::NeedsAuth),
                    other => other,
                }
            }
            other => other,
        }
    }

    fn account(&self) -> Option<ProviderAccount> {
        let credential = self.credential(false).ok()?;
        Some(ProviderAccount { email: credential.email, plan: credential.membership })
    }

    fn forget_cached_credential(&self) {
        *self.cached.write().unwrap_or_else(|e| e.into_inner()) = None;
    }

    fn sign_in_hint(&self) -> SignInHint {
        SignInHint::OpenApp { app: "Cursor".into() }
    }

    fn manage_url(&self) -> &'static str {
        MANAGE_URL
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = include_str!("../../tests/fixtures/cursor_usage_ok.json");
    const NO_ON_DEMAND: &str =
        include_str!("../../tests/fixtures/cursor_usage_no_on_demand.json");
    const EMPTY: &str = include_str!("../../tests/fixtures/cursor_usage_empty.json");

    fn parse(body: &str) -> Result<ProviderSnapshot, ProviderError> {
        parse_usage(body, Utc::now(), None, None)
    }

    #[test]
    fn parses_the_summary_with_the_plan_window_first() {
        let snapshot = parse(OK).expect("fixture should parse");
        assert_eq!(snapshot.provider, ProviderId::Cursor);
        assert_eq!(snapshot.fidelity, Fidelity::Official);

        let ids: Vec<&str> = snapshot.windows.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, ["plan", "api", "on_demand"]);

        // Cursor has no `session` window, so the ring shows the plan.
        let main = snapshot.primary_window().expect("plan window");
        assert_eq!(main.id, "plan");
        assert!((main.used_fraction - 0.375).abs() < 1e-9);
        assert!(main.resets_at.is_some(), "billing cycle end becomes the reset time");

        let on_demand = snapshot.windows.iter().find(|w| w.id == "on_demand").unwrap();
        // 4.31 of 20.00 spent.
        assert!((on_demand.used_fraction - 0.2155).abs() < 1e-9);
    }

    #[test]
    fn the_plan_from_the_response_wins_over_the_one_in_the_database() {
        let snapshot =
            parse_usage(OK, Utc::now(), Some("me@example.com".into()), Some("free".into()))
                .unwrap();
        let account = snapshot.account.expect("account");
        assert_eq!(account.email.as_deref(), Some("me@example.com"));
        assert_eq!(account.plan.as_deref(), Some("pro"));
    }

    #[test]
    fn the_database_plan_is_used_when_the_response_omits_it() {
        let snapshot =
            parse_usage(NO_ON_DEMAND, Utc::now(), None, Some("business".into())).unwrap();
        assert_eq!(snapshot.account.unwrap().plan.as_deref(), Some("business"));
    }

    #[test]
    fn a_missing_on_demand_block_drops_the_window_instead_of_reporting_zero() {
        let snapshot = parse(NO_ON_DEMAND).expect("fixture should parse");
        let ids: Vec<&str> = snapshot.windows.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, ["plan"]);
    }

    #[test]
    fn a_zero_on_demand_limit_is_dropped_rather_than_divided_by() {
        let body = r#"{"individualUsage":{"plan":{"totalPercentUsed":10},
            "onDemand":{"used":0,"limit":0}}}"#;
        let snapshot = parse(body).expect("plan still parses");
        assert_eq!(snapshot.windows.len(), 1);
    }

    #[test]
    fn a_summary_with_no_window_is_nothing_metered_not_zero() {
        assert!(matches!(parse(EMPTY), Err(ProviderError::NothingMetered(_))));
    }

    #[test]
    fn garbage_is_a_bad_response_never_a_number() {
        assert!(matches!(parse("not json"), Err(ProviderError::BadResponse { status: 200 })));
        assert!(matches!(
            parse(r#"{"individualUsage":42}"#),
            Err(ProviderError::BadResponse { status: 200 })
        ));
    }

    #[test]
    fn percentages_are_clamped_into_the_unit_range() {
        let body = r#"{"individualUsage":{"plan":{"totalPercentUsed":180,"apiPercentUsed":-4}}}"#;
        let snapshot = parse(body).unwrap();
        assert_eq!(snapshot.windows[0].used_fraction, 1.0);
        assert_eq!(snapshot.windows[1].used_fraction, 0.0);
    }

    #[test]
    fn rows_become_a_credential_and_a_missing_token_is_needs_auth() {
        let rows = vec![
            (KEY_ACCESS_TOKEN.to_string(), "\"token-value\"".to_string()),
            (KEY_ACCOUNT_ID.to_string(), "user_123".to_string()),
            (KEY_EMAIL.to_string(), "\"me@example.com\"".to_string()),
            (KEY_MEMBERSHIP.to_string(), "\"pro\"".to_string()),
        ];
        let credential = credential_from_rows(&rows).expect("rows should parse");
        assert_eq!(credential.email.as_deref(), Some("me@example.com"));
        assert_eq!(credential.membership.as_deref(), Some("pro"));
        // Exactly the cookie Cursor's own client sends.
        assert_eq!(
            credential.cookie(),
            "WorkosCursorSessionToken=user_123::token-value"
        );

        assert!(matches!(credential_from_rows(&[]), Err(ProviderError::NeedsAuth)));
        let only_email = vec![(KEY_EMAIL.to_string(), "\"me@example.com\"".to_string())];
        assert!(matches!(credential_from_rows(&only_email), Err(ProviderError::NeedsAuth)));
    }

    #[test]
    fn an_empty_token_counts_as_signed_out() {
        let rows = vec![
            (KEY_ACCESS_TOKEN.to_string(), "\"\"".to_string()),
            (KEY_ACCOUNT_ID.to_string(), "user_123".to_string()),
        ];
        assert!(matches!(credential_from_rows(&rows), Err(ProviderError::NeedsAuth)));
    }

    #[test]
    fn values_are_read_whether_or_not_cursor_json_quoted_them() {
        assert_eq!(unquote("\"quoted\""), "quoted");
        assert_eq!(unquote("bare"), "bare");
        assert_eq!(unquote("  \"padded\"  "), "padded");
    }

    #[test]
    fn the_scratch_directory_is_private_unguessable_and_self_deleting() {
        let first = scratch().expect("scratch should be created");
        let path = first.0.clone();
        assert!(path.exists());

        // Unguessable: two in a row must not collide, or a pre-created path
        // could be waiting for the next one.
        let second = scratch().expect("a second scratch should be created");
        assert_ne!(first.0, second.0);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700, "other users must not be able to look inside");
        }

        drop(first);
        assert!(!path.exists(), "a copy of a credential database must not outlive its read");
    }

    #[test]
    fn an_existing_scratch_path_is_refused_rather_than_reused() {
        // Reusing a path somebody else created — a symlink, say — would copy
        // the credential database somewhere they can read it.
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let taken = std::env::temp_dir().join(format!("gaugecode-cursor-taken-{unique:x}"));
        std::fs::create_dir(&taken).unwrap();
        assert!(std::fs::create_dir(&taken).is_err(), "create_dir must reject an existing path");
        let _ = std::fs::remove_dir_all(&taken);
    }

    #[test]
    fn a_copy_is_readable_and_leaves_nothing_behind() {
        let dir = std::env::temp_dir().join(format!("gaugecode-src-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("state.vscdb");

        let seed = Connection::open(&source).unwrap();
        seed.execute_batch(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT);
             INSERT INTO ItemTable VALUES ('cursorAuth/accessToken', '\"tok\"');
             INSERT INTO ItemTable VALUES ('cursorAuth/stripeMembershipAuthId', 'user_7');",
        )
        .unwrap();
        drop(seed);

        let before = scratch_dirs();
        let rows = read_item_table_from_copy(&source).expect("the copy should be readable");
        assert_eq!(credential_from_rows(&rows).unwrap().account_id, "user_7");
        assert_eq!(scratch_dirs(), before, "the scratch directory should be gone");

        let _ = std::fs::remove_dir_all(dir);
    }

    fn scratch_dirs() -> usize {
        std::fs::read_dir(std::env::temp_dir())
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter(|entry| {
                        entry.file_name().to_string_lossy().starts_with("gaugecode-cursor-")
                    })
                    .count()
            })
            .unwrap_or(0)
    }

    #[test]
    fn the_state_database_is_opened_read_only() {
        // Guards the AGENTS.md rule directly: a writable handle here could
        // corrupt the user's Cursor install.
        let dir = std::env::temp_dir().join(format!("gaugecode-ro-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.vscdb");

        let seed = Connection::open(&path).unwrap();
        seed.execute_batch(
            "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value TEXT);
             INSERT INTO ItemTable VALUES ('cursorAuth/accessToken', '\"tok\"');
             INSERT INTO ItemTable VALUES ('cursorAuth/stripeMembershipAuthId', 'user_9');
             INSERT INTO ItemTable VALUES ('unrelated/key', 'ignored');",
        )
        .unwrap();
        drop(seed);

        let rows = read_item_table(&path).expect("read-only open should succeed");
        assert_eq!(rows.len(), 2, "only the keys we ask for come back");
        let credential = credential_from_rows(&rows).unwrap();
        assert_eq!(credential.account_id, "user_9");

        let read_only = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
        assert!(
            read_only.execute("DELETE FROM ItemTable", []).is_err(),
            "the handle we use must reject writes"
        );

        let _ = std::fs::remove_dir_all(dir);
    }
}
