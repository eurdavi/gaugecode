//! Provider adapters (SPEC §6, §7).
//!
//! **Every network call and every credential read in this app lives inside this
//! module.** CI greps for `reqwest` outside of `src/providers/` and fails the build.
//!
//! Adding a provider = one file here (`claude.rs`, `cursor.rs`, `codex.rs`),
//! registered in [`all`]. The UI iterates over snapshots; it never needs to change.
//!
//! Rules that do not bend:
//! * never write to a credential file; SQLite opens read-only;
//! * never log a token, cookie or `Authorization` header;
//! * never produce a percentage that did not come from the response.

use async_trait::async_trait;

use crate::model::{Fidelity, ProviderAccount, ProviderError, ProviderId, ProviderSnapshot, SignInHint};

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
}

/// Registry of every adapter the app knows about, in display order.
///
/// M0: empty. M1 registers Claude; M3 registers Cursor and Codex.
pub fn all() -> Vec<Box<dyn UsageProvider>> {
    Vec::new()
}
