//! The app's single piece of shared state, plus the scheduler's timing rules
//! (SPEC §8). One `RwLock` for everything, per AGENTS.md — never held across an
//! `await`.

use std::collections::HashMap;
use std::sync::RwLock;
use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::i18n::Language;
use crate::model::{Band, Fidelity, ProviderId, ProviderSnapshot, ProviderStatus};
use crate::notch::{NotchAnimation, NotchEdge};
use crate::store::{BackoffRecord, Prefs, Store};

// The polling cadence is a preference now: see `Prefs::poll_interval` and
// `Prefs::idle_poll_interval`, both clamped so no setting can hammer a vendor.
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

/// Loads the cache, dropping anything a real adapter could not have produced.
///
/// The real adapters only ever emit `Official`. A `Manual` snapshot on disk can
/// only be a demo fixture left behind by an earlier run, and showing yesterday's
/// fake percentage as if it were real is precisely what this app must not do.
fn real_snapshots(store: &Store) -> HashMap<ProviderId, ProviderSnapshot> {
    let mut snapshots = store.load_snapshots();
    let before = snapshots.len();
    snapshots.retain(|_, snapshot| snapshot.fidelity != Fidelity::Manual);
    if snapshots.len() != before {
        tracing::debug!(
            dropped = before - snapshots.len(),
            "discarded demo fixtures from the cache"
        );
        store.save_snapshots(&snapshots);
    }
    snapshots
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
    /// Language the OS is set to, used whenever the user has not picked one.
    /// Resolved once at startup: it cannot change while the app runs.
    system_language: Language,
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
    pub fn load(store: Store, demo: bool, system_language: Language) -> Self {
        let mut prefs = store.load_prefs();
        if demo {
            // A demo shows all three rings. In-memory only — `save_prefs` is a
            // no-op in demo mode, so this never touches the real preferences.
            for id in ProviderId::ALL {
                prefs.enabled.insert(id, true);
            }
        }

        let inner = Inner {
            snapshots: if demo { HashMap::new() } else { real_snapshots(&store) },
            statuses: HashMap::new(),
            backoff: if demo { HashMap::new() } else { store.load_backoff() },
            prefs,
            last_face: None,
        };
        Self {
            store,
            demo,
            system_language,
            inner: RwLock::new(inner),
            refresh: tokio::sync::Notify::new(),
        }
    }

    pub fn is_demo(&self) -> bool {
        self.demo
    }

    /// The language to draw in: the user's choice, or the OS's when there is none.
    pub fn language(&self) -> Language {
        self.read().prefs.language.unwrap_or(self.system_language)
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

    /// Mutates the preferences under the write lock and persists the result.
    /// Demo runs skip the disk write (SPEC §9.4).
    fn update_prefs(&self, mutate: impl FnOnce(&mut Inner)) {
        let prefs = {
            let mut inner = self.write();
            mutate(&mut inner);
            inner.prefs.clone()
        };
        self.persist_prefs(&prefs);
    }

    pub fn set_notch_visible(&self, visible: bool) {
        self.update_prefs(|inner| inner.prefs.notch_visible = visible);
    }

    pub fn set_notch_edge(&self, edge: NotchEdge) {
        self.update_prefs(|inner| inner.prefs.notch_edge = edge);
    }

    pub fn set_notch_animation(&self, animation: NotchAnimation) {
        self.update_prefs(|inner| inner.prefs.notch_animation = animation);
    }

    /// `None` goes back to following the operating system.
    pub fn set_language(&self, language: Option<Language>) {
        self.update_prefs(|inner| inner.prefs.language = language);
    }

    pub fn set_autostart(&self, enabled: bool) {
        self.update_prefs(|inner| inner.prefs.autostart = enabled);
    }

    pub fn set_auto_update(&self, enabled: bool) {
        self.update_prefs(|inner| inner.prefs.auto_update = enabled);
    }

    pub fn set_poll_seconds(&self, seconds: u64) {
        self.update_prefs(|inner| inner.prefs.poll_seconds = seconds);
    }

    pub fn set_onboarded(&self, onboarded: bool) {
        self.update_prefs(|inner| inner.prefs.onboarded = onboarded);
    }

    /// Shows or hides one limit window. The tray may need a redraw because the
    /// headline window can change.
    pub fn set_window_hidden(&self, id: ProviderId, window_id: String, hidden: bool) {
        self.update_prefs(|inner| {
            let entry = inner.prefs.hidden_windows.entry(id).or_default();
            if hidden {
                if !entry.contains(&window_id) {
                    entry.push(window_id);
                }
            } else {
                entry.retain(|existing| existing != &window_id);
            }
            inner.last_face = None;
        });
    }

    pub fn set_primary(&self, id: ProviderId) {
        self.update_prefs(|inner| {
            inner.prefs.primary = id;
            // Force the next tray redraw even if the number is the same.
            inner.last_face = None;
        });
    }

    pub fn snapshot(&self, id: ProviderId) -> Option<ProviderSnapshot> {
        self.read().snapshots.get(&id).cloned()
    }

    /// The snapshot as the user chose to see it, with hidden windows removed.
    ///
    /// The filter lives here rather than in the UI so the tray, the notch and
    /// the popup can never disagree about which window is the headline one.
    /// Hiding every window would leave nothing to draw, so that degenerate case
    /// is ignored and the full snapshot comes back.
    pub fn visible_snapshot(&self, id: ProviderId) -> Option<ProviderSnapshot> {
        let inner = self.read();
        Self::visible_snapshot_of(&inner, id)
    }

    fn visible_snapshot_of(inner: &Inner, id: ProviderId) -> Option<ProviderSnapshot> {
        let mut snapshot = inner.snapshots.get(&id).cloned()?;
        let kept: Vec<_> = snapshot
            .windows
            .iter()
            .filter(|window| !inner.prefs.is_window_hidden(id, &window.id))
            .cloned()
            .collect();
        if !kept.is_empty() {
            snapshot.windows = kept;
        }
        Some(snapshot)
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

    /// The provider whose number the tray icon shows.
    ///
    /// Normally the one the user picked. When that one has nothing to show — not
    /// signed in, or still on its first fetch — the icon borrows the first
    /// enabled provider that does have a reading, rather than sitting on a dash
    /// while another provider has a perfectly good number. The tooltip names
    /// whichever provider this returns, so the two can never disagree.
    pub fn tray_provider(&self) -> ProviderId {
        let inner = self.read();
        Self::tray_provider_of(&inner)
    }

    fn tray_provider_of(inner: &Inner) -> ProviderId {
        let has_reading = |id: ProviderId| {
            inner.prefs.is_enabled(id)
                && Self::visible_snapshot_of(inner, id)
                    .and_then(|snapshot| snapshot.primary_window().cloned())
                    .is_some()
        };

        let primary = inner.prefs.primary;
        if has_reading(primary) {
            return primary;
        }
        ProviderId::ALL.into_iter().find(|id| has_reading(*id)).unwrap_or(primary)
    }

    fn face(&self, inner: &Inner) -> TrayFace {
        let id = Self::tray_provider_of(inner);
        if !inner.prefs.is_enabled(id) {
            return TrayFace { percent: None, band: Band::Off, dimmed: false };
        }
        let window = Self::visible_snapshot_of(inner, id)
            .and_then(|snapshot| snapshot.primary_window().cloned());
        let Some(window) = window else {
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

    fn snapshot_of(provider: ProviderId) -> ProviderSnapshot {
        ProviderSnapshot {
            provider,
            fidelity: Fidelity::Official,
            windows: vec![crate::model::LimitWindow {
                id: "session".into(),
                label: "Session".into(),
                used_fraction: 0.4,
                resets_at: None,
            }],
            fetched_at: Utc::now(),
            account: None,
        }
    }

    fn inner_with(primary: ProviderId, readings: &[ProviderId]) -> Inner {
        Inner {
            prefs: Prefs { primary, ..Prefs::default() },
            snapshots: readings.iter().map(|id| (*id, snapshot_of(*id))).collect(),
            ..Inner::default()
        }
    }

    #[test]
    fn the_tray_shows_the_chosen_provider_whenever_it_has_a_reading() {
        let inner = inner_with(ProviderId::Claude, &[ProviderId::Claude, ProviderId::Cursor]);
        assert_eq!(AppState::tray_provider_of(&inner), ProviderId::Claude);
    }

    #[test]
    fn the_tray_borrows_another_provider_rather_than_showing_a_dash() {
        // The real case here: Claude is picked but has no subscription login,
        // while Cursor is reporting fine.
        let inner = inner_with(ProviderId::Claude, &[ProviderId::Cursor]);
        assert_eq!(AppState::tray_provider_of(&inner), ProviderId::Cursor);
    }

    #[test]
    fn with_no_reading_anywhere_the_tray_stays_on_the_chosen_provider() {
        let inner = inner_with(ProviderId::Cursor, &[]);
        assert_eq!(AppState::tray_provider_of(&inner), ProviderId::Cursor);
    }

    #[test]
    fn a_disabled_provider_is_never_borrowed() {
        let mut inner = inner_with(ProviderId::Claude, &[ProviderId::Cursor]);
        inner.prefs.enabled.insert(ProviderId::Cursor, false);
        assert_eq!(AppState::tray_provider_of(&inner), ProviderId::Claude);
    }

    #[test]
    fn a_demo_fixture_left_in_the_cache_is_discarded_on_a_real_launch() {
        // This happened for real: a demo run from before the no-persist guard
        // left `manual` snapshots in the store, and they would have been shown
        // as if they came from the vendor.
        let dir = std::env::temp_dir().join(format!("gaugecode-fixture-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = Store::new(dir.clone());

        let mut fixture = snapshot_of(ProviderId::Claude);
        fixture.fidelity = Fidelity::Manual;
        store.save_snapshots(&HashMap::from([
            (ProviderId::Claude, fixture),
            (ProviderId::Cursor, snapshot_of(ProviderId::Cursor)),
        ]));

        let kept = real_snapshots(&store);
        assert_eq!(kept.keys().copied().collect::<Vec<_>>(), vec![ProviderId::Cursor]);
        // And the cleaned cache is written back, so it stays gone.
        assert_eq!(store.load_snapshots().len(), 1);

        let _ = std::fs::remove_dir_all(dir);
    }
}
