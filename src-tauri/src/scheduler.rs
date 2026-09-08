//! Adaptive polling, 429 backoff and event fan-out (SPEC §8).
//!
//! One task per provider. Each task fetches, publishes and then waits — either
//! for its interval or for a manual refresh, whichever comes first.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};
use tauri::{AppHandle, Emitter, Manager};

use crate::model::{ProviderError, ProviderId, ProviderStatus};
use crate::providers::Registry;
use crate::state::{AppState, JITTER};
use crate::tray;

pub const SNAPSHOT_EVENT: &str = "usage:snapshot";
pub const STATUS_EVENT: &str = "usage:status";
/// A second fetch inside this window is ignored, so looping `refresh_now`
/// cannot spend the user's own rate limit.
const MIN_FETCH_GAP: Duration = Duration::from_secs(10);

#[derive(Clone, Serialize)]
struct StatusPayload {
    provider: ProviderId,
    status: ProviderStatus,
}

pub fn spawn(app: &AppHandle) {
    let state = app.state::<Arc<AppState>>().inner().clone();
    let registry = app.state::<Arc<Registry>>().inner().clone();

    for (index, id) in ProviderId::ALL.into_iter().enumerate() {
        let app = app.clone();
        let state = state.clone();
        let registry = registry.clone();
        // Stagger the providers so the three never hit the network on the same second.
        let stagger = JITTER * index as u32;

        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(stagger).await;
            loop {
                tick(&app, &state, &registry, id).await;
                let delay = next_delay(&state, id);
                tracing::debug!(provider = ?id, delay_secs = delay.as_secs(), "sleeping");
                tokio::select! {
                    _ = tokio::time::sleep(delay) => {}
                    _ = state.wait_for_refresh() => {
                        tracing::debug!(provider = ?id, "manual refresh requested");
                    }
                }
            }
        });
    }
}

async fn tick(app: &AppHandle, state: &Arc<AppState>, registry: &Arc<Registry>, id: ProviderId) {
    if !state.prefs().is_enabled(id) {
        publish(app, state, id, ProviderStatus::Disabled);
        return;
    }

    // A penalty that outlived a restart still has to be served (SPEC §8).
    if let Some(until) = state.backoff_until(id) {
        tracing::debug!(provider = ?id, until = %until, "still in backoff; not spending an attempt");
        let message = state.rate_limited_notice(id).unwrap_or_else(|| "rate limited".into());
        let status = state.stale_or_error(id, message);
        publish(app, state, id, status);
        return;
    }

    if state.fetched_within(id, MIN_FETCH_GAP) {
        tracing::debug!(provider = ?id, "skipping fetch; last attempt was seconds ago");
        return;
    }

    // A provider without an adapter in this build stays without a reading,
    // rather than being given an invented one.
    let Some(provider) = registry.get(id) else { return };

    state.note_fetch(id);
    match provider.fetch_snapshot().await {
        Ok(snapshot) => {
            state.clear_backoff(id);
            state.record_snapshot(snapshot);
            // Emit the snapshot as the user chose to see it, not as it came off
            // the wire: the UI patches its state from this event, so an
            // unfiltered payload here would undo the per-window choices every
            // cycle.
            if let Some(visible) = state.visible_snapshot(id) {
                let _ = app.emit(SNAPSHOT_EVENT, &visible);
            }
            publish(app, state, id, ProviderStatus::Fresh);
        }
        Err(ProviderError::RateLimited { retry_after }) => {
            let until = state.record_rate_limit(id, retry_after);
            tracing::warn!(
                provider = ?id,
                retry_after_secs = retry_after.as_secs(),
                until = %until,
                "rate limited; backing off"
            );
            let message = state.rate_limited_notice(id).unwrap_or_else(|| "rate limited".into());
            let status = state.stale_or_error(id, message);
            publish(app, state, id, status);
        }
        Err(ProviderError::NeedsAuth) => {
            tracing::info!(provider = ?id, "no credential found");
            publish(app, state, id, ProviderStatus::NeedsAuth);
        }
        Err(error) => {
            tracing::warn!(provider = ?id, %error, "usage fetch failed");
            let status = state.stale_or_error(id, error.to_string());
            publish(app, state, id, status);
        }
    }
}

fn publish(app: &AppHandle, state: &Arc<AppState>, id: ProviderId, status: ProviderStatus) {
    state.record_status(id, status.clone());
    let _ = app.emit(STATUS_EVENT, StatusPayload { provider: id, status });
    tray::update(app);
}

fn next_delay(state: &Arc<AppState>, id: ProviderId) -> Duration {
    if let Some(until) = state.backoff_until(id) {
        return remaining_backoff(until, state.prefs().poll_interval());
    }
    let prefs = state.prefs();
    if state.is_demo() || provider_is_running(id) {
        prefs.poll_interval()
    } else {
        prefs.idle_poll_interval()
    }
}

/// Sleep the leftover penalty, but no longer than one poll interval, so a
/// webview that missed the first `usage:status` still hears about the
/// backoff without us spending a fetch.
fn remaining_backoff(until: DateTime<Utc>, poll: Duration) -> Duration {
    let seconds = until.signed_duration_since(Utc::now()).num_seconds().max(1) as u64;
    Duration::from_secs(seconds).min(poll)
}

/// Cheap "is this tool open?" check (SPEC §8). A false negative only means we
/// poll every 5 min instead of every minute, so an exact match is enough.
fn provider_is_running(id: ProviderId) -> bool {
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing(),
    );

    let wanted = id.process_names();
    let running = system.processes().values().any(|process| {
        let name = process.name().to_string_lossy().to_lowercase();
        wanted.contains(&name.as_str())
    });
    tracing::debug!(provider = ?id, running, "process check");
    running
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_live_penalty_is_reannounced_at_the_poll_cadence() {
        let until = Utc::now() + chrono::Duration::seconds(90);
        assert_eq!(remaining_backoff(until, Duration::from_secs(60)), Duration::from_secs(60));
    }

    #[test]
    fn the_last_slice_of_a_penalty_is_the_time_left() {
        let until = Utc::now() + chrono::Duration::seconds(12);
        let got = remaining_backoff(until, Duration::from_secs(60));
        assert!(got <= Duration::from_secs(12));
        assert!(got >= Duration::from_secs(11));
    }

    #[test]
    fn an_already_elapsed_penalty_still_sleeps_a_second() {
        let until = Utc::now() - chrono::Duration::seconds(5);
        assert_eq!(remaining_backoff(until, Duration::from_secs(60)), Duration::from_secs(1));
    }
}
