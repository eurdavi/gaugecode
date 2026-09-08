//! On-disk persistence in `app_data_dir` (SPEC §11).
//!
//! Three files: `snapshots.json` (last good snapshot per provider),
//! `backoff.json` (rate-limit deadlines) and `prefs.json`. **No token, cookie or
//! credential ever lands here** — those are re-read from their original source
//! every cycle.
//!
//! A corrupt or unreadable file is never fatal: it is logged and treated as
//! empty, because a cache that fails should not stop the app from starting.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::model::{ProviderId, ProviderSnapshot};
use crate::notch::NotchEdge;

const SNAPSHOTS_FILE: &str = "snapshots.json";
const BACKOFF_FILE: &str = "backoff.json";
const PREFS_FILE: &str = "prefs.json";

/// A rate-limit penalty that survives a restart, so relaunching the app during
/// the penalty waits instead of spending another attempt (SPEC §8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackoffRecord {
    pub until: DateTime<Utc>,
    /// Consecutive 429s so far; drives the doubling.
    pub consecutive: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub enabled: HashMap<ProviderId, bool>,
    /// Provider whose session percentage is drawn on the tray icon.
    pub primary: ProviderId,
    pub notch_visible: bool,
    pub notch_edge: NotchEdge,
}

impl Default for Prefs {
    fn default() -> Self {
        // M1 ships the Claude adapter only; Cursor and Codex stay off until M3.
        let enabled = HashMap::from([(ProviderId::Claude, true)]);
        Self {
            enabled,
            primary: ProviderId::Claude,
            notch_visible: true,
            notch_edge: NotchEdge::default(),
        }
    }
}

impl Prefs {
    pub fn is_enabled(&self, id: ProviderId) -> bool {
        self.enabled.get(&id).copied().unwrap_or(false)
    }
}

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        if let Err(error) = fs::create_dir_all(&dir) {
            tracing::warn!(%error, "could not create app data dir; state will not persist");
        }
        Self { dir }
    }

    pub fn load_snapshots(&self) -> HashMap<ProviderId, ProviderSnapshot> {
        self.read(SNAPSHOTS_FILE)
    }

    pub fn save_snapshots(&self, value: &HashMap<ProviderId, ProviderSnapshot>) {
        self.write(SNAPSHOTS_FILE, value);
    }

    pub fn load_backoff(&self) -> HashMap<ProviderId, BackoffRecord> {
        self.read(BACKOFF_FILE)
    }

    pub fn save_backoff(&self, value: &HashMap<ProviderId, BackoffRecord>) {
        self.write(BACKOFF_FILE, value);
    }

    pub fn load_prefs(&self) -> Prefs {
        self.read(PREFS_FILE)
    }

    pub fn save_prefs(&self, value: &Prefs) {
        self.write(PREFS_FILE, value);
    }

    fn read<T: DeserializeOwned + Default>(&self, name: &str) -> T {
        let path = self.dir.join(name);
        let raw = match fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return T::default(),
            Err(error) => {
                tracing::warn!(file = name, %error, "could not read state file");
                return T::default();
            }
        };
        match serde_json::from_str(&raw) {
            Ok(value) => value,
            Err(error) => {
                tracing::warn!(file = name, %error, "discarding corrupt state file");
                T::default()
            }
        }
    }

    fn write<T: Serialize>(&self, name: &str, value: &T) {
        let json = match serde_json::to_vec_pretty(value) {
            Ok(json) => json,
            Err(error) => {
                tracing::warn!(file = name, %error, "could not serialize state");
                return;
            }
        };

        // Write beside the target and swap, so a crash mid-write cannot leave a
        // half-written file behind. `rename` refuses to clobber on Windows, hence
        // the remove first.
        let tmp = self.dir.join(format!("{name}.tmp"));
        let dest = self.dir.join(name);
        if let Err(error) = fs::write(&tmp, &json) {
            tracing::warn!(file = name, %error, "could not write state file");
            return;
        }
        let _ = fs::remove_file(&dest);
        if let Err(error) = fs::rename(&tmp, &dest) {
            tracing::warn!(file = name, %error, "could not swap state file into place");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Fidelity, LimitWindow};

    /// Tests run in parallel, so each one needs its own directory.
    fn temp_store(name: &str) -> (Store, PathBuf) {
        let dir = std::env::temp_dir()
            .join(format!("gaugecode-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        (Store::new(dir.clone()), dir)
    }

    #[test]
    fn missing_files_read_as_defaults() {
        let (store, dir) = temp_store("missing");
        assert!(store.load_snapshots().is_empty());
        assert!(store.load_backoff().is_empty());
        assert_eq!(store.load_prefs(), Prefs::default());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn snapshots_round_trip_and_corrupt_files_are_discarded() {
        let (store, dir) = temp_store("round-trip");

        let snapshot = ProviderSnapshot {
            provider: ProviderId::Claude,
            fidelity: Fidelity::Official,
            windows: vec![LimitWindow {
                id: "session".into(),
                label: "Session (5h)".into(),
                used_fraction: 0.42,
                resets_at: None,
            }],
            fetched_at: Utc::now(),
            account: None,
        };
        let map = HashMap::from([(ProviderId::Claude, snapshot.clone())]);
        store.save_snapshots(&map);
        assert_eq!(store.load_snapshots(), map);

        fs::write(dir.join(SNAPSHOTS_FILE), b"{ not json").unwrap();
        assert!(store.load_snapshots().is_empty());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn prefs_default_enables_claude_only() {
        let prefs = Prefs::default();
        assert!(prefs.is_enabled(ProviderId::Claude));
        assert!(!prefs.is_enabled(ProviderId::Cursor));
        assert!(!prefs.is_enabled(ProviderId::Codex));
        assert_eq!(prefs.primary, ProviderId::Claude);
        assert_eq!(prefs.notch_edge, NotchEdge::Right);
    }

    #[test]
    fn prefs_written_before_the_notch_existed_still_load() {
        // `#[serde(default)]` keeps an old prefs.json readable instead of
        // throwing away the user's provider choices.
        let (store, dir) = temp_store("old-prefs");
        fs::write(dir.join(PREFS_FILE), br#"{"enabled":{"claude":true},"primary":"claude"}"#)
            .unwrap();
        let prefs = store.load_prefs();
        assert!(prefs.is_enabled(ProviderId::Claude));
        assert_eq!(prefs.notch_edge, NotchEdge::Right);
        assert!(prefs.notch_visible);
        let _ = fs::remove_dir_all(dir);
    }
}
