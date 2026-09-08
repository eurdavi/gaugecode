//! The app's single piece of shared state, plus the scheduler's timing rules
//! (SPEC §8). One `RwLock` for everything, per AGENTS.md — never held across an
//! `await`.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::model::{Band, ProviderId, ProviderSnapshot, ProviderStatus};
use crate::notch::NotchEdge;
use crate::store::{BackoffRecord, Prefs, Store};

/// Polling cadence while the provider's tool is running.
pub const POLL_INTERVAL: Duration = Duration::from_secs(60);
/// Cadence when no process of that tool is found.
pub const IDLE_POLL_INTERVAL: Duration = Duration::from_secs(5 * 60);
pub const BACKOFF_BASE: Duration = Duration::from_secs(60);
pub const BACKOFF_CAP: Duration = Duration::from_secs(15 * 60);
/// Spread between providers so the three never fire in the same second.
pub const JITTER: Duration = Duration::from_secs(5);

/// 60 s, doubling on every consecutive 429, capped at 15 min. `Retry-After` can
/// only raise the floor — the Claude endpoint answers `Retry-After: 0`, which
/// must not shorten the penalty (SPEC §8).
pub fn backoff_delay(consecutive: u32, retry_after: Option<Duration>) -> Duration {
    let doublings = consecutive.saturating_sub(1).min(8);
    let grown = BACKOFF_BASE.saturating_mul(1u32 << doublings);
    let capped = grown.min(BACKOFF_CAP);
    match retry_after {
        Some(after) if after > capped => after,
        _ => capped,
    }
}

/// What the tray icon should currently draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrayFace {
    /// `None` when there is no number to show; the icon draws a dash.
    pub percent: Option<u8>,
    pub band: Band,
    pub dimmed: bool,
}

pub struct AppState {
    store: Store,
    demo: bool,
    inner: RwLock<Inner>,
    refresh: tokio::sync::Notify,
}

#[derive(Default)]
struct Inner {
    snapshots: HashMap<ProviderId, ProviderSnapshot>,
    /// `None` for a provider means "no reading yet", which the UI shows as a
    /// placeholder rather than inventing a status.
    statuses: HashMap<ProviderId, ProviderStatus>,
    backoff: HashMap<ProviderId, BackoffRecord>,
    prefs: Prefs,
    last_face: Option<TrayFace>,
}

impl AppState {
    pub fn load(store: Store, demo: bool) -> Self {
        let mut prefs = store.load_prefs();
        if demo {
            // A demo shows all three rings. In-memory only — `save_prefs` is a
            // no-op in demo mode, so this never touches the real preferences.
            for id in ProviderId::ALL {
                prefs.enabled.insert(id, true);
            }
        }

        let inner = Inner {
            snapshots: if demo { HashMap::new() } else { store.load_snapshots() },
            statuses: HashMap::new(),
            backoff: if demo { HashMap::new() } else { store.load_backoff() },
            prefs,
            last_face: None,
        };
        Self { store, demo, inner: RwLock::new(inner), refresh: tokio::sync::Notify::new() }
    }

    pub fn is_demo(&self) -> bool {
        self.demo
    }

    /// Demo runs must not overwrite the real cache or preferences (SPEC §9.4).
    fn persist_prefs(&self, prefs: &Prefs) {
        if !self.demo {
            self.store.save_prefs(prefs);
        }
    }

    fn persist_snapshots(&self, snapshots: &HashMap<ProviderId, ProviderSnapshot>) {
        if !self.demo {
            self.store.save_snapshots(snapshots);
        }
    }

    fn persist_backoff(&self, backoff: &HashMap<ProviderId, BackoffRecord>) {
        if !self.demo {
            self.store.save_backoff(backoff);
        }
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, Inner> {
        // A poisoned lock means another thread panicked while holding it. The
        // state is a cache, so recovering is better than cascading the panic.
        self.inner.read().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, Inner> {
        self.inner.write().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn prefs(&self) -> Prefs {
        self.read().prefs.clone()
    }

    pub fn set_provider_enabled(&self, id: ProviderId, enabled: bool) {
        let prefs = {
            let mut inner = self.write();
            inner.prefs.enabled.insert(id, enabled);
            if !enabled {
                // Turning a provider off must drop its number, so it does not
                // come back on the next launch (SPEC §9.3).
                inner.snapshots.remove(&id);
                inner.backoff.remove(&id);
                inner.statuses.insert(id, ProviderStatus::Disabled);
            } else {
                inner.statuses.remove(&id);
            }
            let snapshots = inner.snapshots.clone();
            let backoff = inner.backoff.clone();
            self.persist_snapshots(&snapshots);
            self.persist_backoff(&backoff);
            inner.prefs.clone()
        };
        self.persist_prefs(&prefs);
    }

    pub fn set_notch_visible(&self, visible: bool) {
        let prefs = {
            let mut inner = self.write();
            inner.prefs.notch_visible = visible;
            inner.prefs.clone()
        };
        self.persist_prefs(&prefs);
    }

    pub fn set_notch_edge(&self, edge: NotchEdge) {
        let prefs = {
            let mut inner = self.write();
            inner.prefs.notch_edge = edge;
            inner.prefs.clone()
        };
        self.persist_prefs(&prefs);
    }

    pub fn set_primary(&self, id: ProviderId) {
        let prefs = {
            let mut inner = self.write();
            inner.prefs.primary = id;
            // Force the next tray redraw even if the number is the same.
            inner.last_face = None;
            inner.prefs.clone()
        };
        self.persist_prefs(&prefs);
    }

    pub fn snapshot(&self, id: ProviderId) -> Option<ProviderSnapshot> {
        self.read().snapshots.get(&id).cloned()
    }

    pub fn status(&self, id: ProviderId) -> Option<ProviderStatus> {
        self.read().statuses.get(&id).cloned()
    }

    pub fn record_snapshot(&self, snapshot: ProviderSnapshot) {
        let snapshots = {
            let mut inner = self.write();
            inner.snapshots.insert(snapshot.provider, snapshot);
            inner.snapshots.clone()
        };
        self.persist_snapshots(&snapshots);
    }

    pub fn record_status(&self, id: ProviderId, status: ProviderStatus) {
        self.write().statuses.insert(id, status);
    }

    /// Status to show when a fetch failed: the old snapshot with its age when we
    /// have one, an error otherwise. Never a zero.
    pub fn stale_or_error(&self, id: ProviderId, message: String) -> ProviderStatus {
        match self.snapshot(id) {
            Some(snapshot) => {
                let age = Utc::now().signed_duration_since(snapshot.fetched_at);
                ProviderStatus::Stale { age_secs: age.num_seconds().max(0) as u64 }
            }
            None => ProviderStatus::Error { message },
        }
    }

    pub fn backoff_until(&self, id: ProviderId) -> Option<DateTime<Utc>> {
        self.read().backoff.get(&id).map(|record| record.until).filter(|until| *until > Utc::now())
    }

    /// Registers a 429 and returns the moment the penalty ends.
    pub fn record_rate_limit(&self, id: ProviderId, retry_after: Duration) -> DateTime<Utc> {
        let (until, backoff) = {
            let mut inner = self.write();
            let consecutive = inner.backoff.get(&id).map_or(0, |record| record.consecutive) + 1;
            let delay = backoff_delay(consecutive, Some(retry_after));
            let until = Utc::now() + chrono::Duration::from_std(delay).unwrap_or_default();
            inner.backoff.insert(id, BackoffRecord { until, consecutive });
            (until, inner.backoff.clone())
        };
        self.persist_backoff(&backoff);
        until
    }

    pub fn clear_backoff(&self, id: ProviderId) {
        let backoff = {
            let mut inner = self.write();
            if inner.backoff.remove(&id).is_none() {
                return;
            }
            inner.backoff.clone()
        };
        self.persist_backoff(&backoff);
    }

    /// Wakes every provider loop (used by `refresh_now`).
    pub fn request_refresh(&self) {
        self.refresh.notify_waiters();
    }

    pub async fn wait_for_refresh(&self) {
        self.refresh.notified().await;
    }

    fn face(&self, inner: &Inner) -> TrayFace {
        let id = inner.prefs.primary;
        if !inner.prefs.is_enabled(id) {
            return TrayFace { percent: None, band: Band::Off, dimmed: false };
        }
        let Some(window) = inner.snapshots.get(&id).and_then(ProviderSnapshot::primary_window) else {
            return TrayFace { percent: None, band: Band::Off, dimmed: false };
        };
        let percent = (window.used_fraction * 100.0).round().clamp(0.0, 100.0) as u8;
        let dimmed = inner.statuses.get(&id).is_none_or(ProviderStatus::is_dimmed);
        TrayFace { percent: Some(percent), band: Band::of(window.used_fraction), dimmed }
    }

    /// Returns the face only when it differs from the last one drawn, so the icon
    /// is re-rendered only when the whole percent changes (SPEC §9.2).
    pub fn face_if_changed(&self) -> Option<TrayFace> {
        let mut inner = self.write();
        let face = self.face(&inner);
        if inner.last_face == Some(face) {
            return None;
        }
        inner.last_face = Some(face);
        Some(face)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_from_a_minute_and_stops_at_fifteen() {
        let delay = |n| backoff_delay(n, None).as_secs();
        assert_eq!(delay(1), 60);
        assert_eq!(delay(2), 120);
        assert_eq!(delay(3), 240);
        assert_eq!(delay(4), 480);
        assert_eq!(delay(5), 900);
        assert_eq!(delay(6), 900);
        assert_eq!(delay(50), 900);
    }

    #[test]
    fn retry_after_is_a_floor_and_never_shortens_the_penalty() {
        // The Claude endpoint answers `Retry-After: 0`.
        assert_eq!(backoff_delay(1, Some(Duration::ZERO)).as_secs(), 60);
        assert_eq!(backoff_delay(1, Some(Duration::from_secs(30))).as_secs(), 60);
        // A longer hint wins.
        assert_eq!(backoff_delay(1, Some(Duration::from_secs(120))).as_secs(), 120);
        assert_eq!(backoff_delay(9, Some(Duration::from_secs(3600))).as_secs(), 3600);
    }

    #[test]
    fn zero_consecutive_still_waits_the_base_delay() {
        assert_eq!(backoff_delay(0, None).as_secs(), 60);
    }
}
