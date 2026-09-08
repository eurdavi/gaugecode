//! Demo provider for `GAUGECODE_DEMO=1` (SPEC §9.4).
//!
//! Fixed numbers, no network, no credential read — so the UI can be recorded or
//! reviewed without exposing a real account. It reports
//! [`Fidelity::Manual`] so nothing here can ever be mistaken for a real reading.

use async_trait::async_trait;
use chrono::{Duration, Utc};

use crate::model::{
    Fidelity, LimitWindow, ProviderAccount, ProviderError, ProviderId, ProviderSnapshot, SignInHint,
};
use crate::providers::UsageProvider;

pub struct DemoProvider {
    id: ProviderId,
}

impl DemoProvider {
    pub fn new(id: ProviderId) -> Self {
        Self { id }
    }

    fn windows(&self) -> Vec<(&'static str, &'static str, f64, i64)> {
        match self.id {
            ProviderId::Claude => vec![
                ("session", "Session (5h)", 0.42, 137),
                ("weekly_all", "Weekly (all models)", 0.67, 4_320),
                ("weekly_opus", "Weekly (Opus)", 0.18, 4_320),
            ],
            ProviderId::Cursor => vec![
                ("session", "Plan usage", 0.78, 11_500),
                ("on_demand", "On-demand", 0.24, 11_500),
            ],
            ProviderId::Codex => vec![
                ("primary", "Primary window", 0.91, 46),
                ("secondary", "Secondary window", 0.33, 8_640),
            ],
            ProviderId::Glm => vec![
                ("session", "Current session", 0.12, 210),
                ("weekly", "Weekly", 0.55, 5_760),
            ],
            ProviderId::Grok => vec![("credits", "Grok Build", 0.08, 7_200)],
            ProviderId::OpenCode => vec![
                ("rolling", "5h limit", 0.63, 95),
                ("weekly", "Weekly limit", 0.41, 6_100),
                ("monthly", "Monthly limit", 0.15, 24_000),
            ],
        }
    }
}

#[async_trait]
impl UsageProvider for DemoProvider {
    fn id(&self) -> ProviderId {
        self.id
    }

    fn fidelity(&self) -> Fidelity {
        Fidelity::Manual
    }

    async fn fetch_snapshot(&self) -> Result<ProviderSnapshot, ProviderError> {
        let now = Utc::now();
        let windows = self
            .windows()
            .into_iter()
            .map(|(id, label, used_fraction, resets_in_minutes)| LimitWindow {
                id: id.to_string(),
                label: label.to_string(),
                used_fraction,
                resets_at: Some(now + Duration::minutes(resets_in_minutes)),
            })
            .collect();

        Ok(ProviderSnapshot {
            provider: self.id,
            fidelity: Fidelity::Manual,
            windows,
            fetched_at: now,
            account: Some(ProviderAccount {
                email: Some("demo@example.com".into()),
                plan: Some("demo".into()),
            }),
        })
    }

    fn account(&self) -> Option<ProviderAccount> {
        Some(ProviderAccount {
            email: Some("demo@example.com".into()),
            plan: Some("demo".into()),
        })
    }

    fn forget_cached_credential(&self) {}

    fn sign_in_hint(&self) -> SignInHint {
        SignInHint::Text { text: "Demo mode: numbers are fixtures.".into() }
    }

    fn manage_url(&self) -> &'static str {
        "https://github.com/eurdavi/gaugecode"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_provider_has_a_primary_window_in_demo_mode() {
        for id in ProviderId::ALL {
            let provider = DemoProvider::new(id);
            let snapshot = tauri::async_runtime::block_on(provider.fetch_snapshot())
                .expect("demo never fails");
            let window = snapshot.primary_window().expect("a primary window");
            assert!((0.0..=1.0).contains(&window.used_fraction));
            assert_eq!(snapshot.fidelity, Fidelity::Manual);
        }
    }
}
