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

use async_trait::async_trait;

use crate::model::{
    Fidelity, ProviderAccount, ProviderError, ProviderId, ProviderSnapshot, SignInHint,
};

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
            ]
        };
        Self { providers }
    }

    pub fn get(&self, id: ProviderId) -> Option<&dyn UsageProvider> {
        self.providers.iter().map(Box::as_ref).find(|provider| provider.id() == id)
    }
}
