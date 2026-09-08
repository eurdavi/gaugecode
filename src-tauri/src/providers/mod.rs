//! Provider adapters (SPEC §6, §7).
//!
//! **Every network call and every credential read in this app lives inside this
//! module.** CI greps for `reqwest` outside of `src/providers/` and fails the build.
//!
//! Adding a provider = one file here, registered in [`Registry::new`]. The UI
//! iterates over snapshots; it never needs to change.
//!
//! Rules that do not bend:
//! * never write to a credential file; SQLite opens read-only;
//! * never log a token, cookie or `Authorization` header;
//! * never produce a percentage that did not come from the response.

pub mod claude;
pub mod codex;
pub mod cursor;
pub mod demo;
pub mod glm;
pub mod grok;
pub mod opencode;

use std::time::Duration;

use async_trait::async_trait;

use crate::model::{
    Fidelity, ProviderAccount, ProviderError, ProviderId, ProviderSnapshot, SignInHint,
};

/// `Retry-After` in seconds, when present and parseable.
///
/// Only ever a floor: the Claude endpoint answers `Retry-After: 0`, which must
/// not shorten a penalty (SPEC §8). Shared because every adapter needs it and
/// they all need it to behave identically.
pub fn retry_after_header(headers: &reqwest::header::HeaderMap) -> Duration {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::ZERO)
}

#[async_trait]
pub trait UsageProvider: Send + Sync {
    fn id(&self) -> ProviderId;
    fn fidelity(&self) -> Fidelity;
    async fn fetch_snapshot(&self) -> Result<ProviderSnapshot, ProviderError>;
    /// Read from the credential, no network.
    fn account(&self) -> Option<ProviderAccount>;
    /// Called when the user turns the provider off.
    fn forget_cached_credential(&self);
    fn sign_in_hint(&self) -> SignInHint;
    /// Where the user manages usage for this provider (SPEC §7 "manage" links).
    fn manage_url(&self) -> &'static str;
}

/// Every adapter the running build knows about.
pub struct Registry {
    providers: Vec<Box<dyn UsageProvider>>,
}

impl Registry {
    /// In demo mode every provider is a fixture, so the UI can be exercised
    /// without touching a credential or the network (SPEC §9.4).
    pub fn new(demo: bool) -> Self {
        let providers: Vec<Box<dyn UsageProvider>> = if demo {
            ProviderId::ALL
                .iter()
                .map(|id| Box::new(demo::DemoProvider::new(*id)) as Box<dyn UsageProvider>)
                .collect()
        } else {
            vec![
                Box::new(claude::ClaudeProvider::new()),
                Box::new(cursor::CursorProvider::new()),
                Box::new(codex::CodexProvider::new()),
                Box::new(glm::GlmProvider::new()),
                Box::new(grok::GrokProvider::new()),
                Box::new(opencode::OpenCodeProvider::new()),
            ]
        };
        Self { providers }
    }

    pub fn get(&self, id: ProviderId) -> Option<&dyn UsageProvider> {
        self.providers.iter().map(Box::as_ref).find(|provider| provider.id() == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_provider_has_an_adapter_in_a_real_build() {
        let registry = Registry::new(false);
        for id in ProviderId::ALL {
            assert!(registry.get(id).is_some(), "{id:?} has no adapter registered");
        }
    }

    #[test]
    fn retry_after_is_parsed_and_defaults_to_zero() {
        // Only ever a floor: the Claude endpoint answers `Retry-After: 0`.
        let mut headers = reqwest::header::HeaderMap::new();
        assert_eq!(retry_after_header(&headers), Duration::ZERO);
        headers.insert(reqwest::header::RETRY_AFTER, "0".parse().unwrap());
        assert_eq!(retry_after_header(&headers), Duration::ZERO);
        headers.insert(reqwest::header::RETRY_AFTER, "90".parse().unwrap());
        assert_eq!(retry_after_header(&headers), Duration::from_secs(90));
        // An HTTP-date form is not parsed, and must not panic.
        headers.insert(
            reqwest::header::RETRY_AFTER,
            "Wed, 21 Oct 2026 07:28:00 GMT".parse().unwrap(),
        );
        assert_eq!(retry_after_header(&headers), Duration::ZERO);
    }
}
