use std::sync::Arc;
use webb_proposals::ProposalTrait;

#[doc(hidden)]
mod dkg;
#[doc(hidden)]
mod mocked;

/// A module that Implements the DKG Proposal Signing Backend.
pub use dkg::*;
/// A module that Implements the Mocked Proposal Signing Backend.
pub use mocked::*;
use crate::metric;

/// A Proposal Signing Backend is responsible for signing proposal `P` where `P` is anything really depending on the
/// requirement of the user of this backend.
///
/// For example, an Anchor Event Watcher that watches for `Deposit` events might need to sign an `AnchorUpdateProposal` and to do so, it will
/// require a `ProposalSigningBackend<AnchorUpdateProposal>` to do so.
///
/// As of now, we have two implementations of this trait:
///
/// - `DkgSigningBackend`: This is using the `DKG` protocol to sign the proposal.
/// - `MockedSigningBackend`: This is using the Governor's `PrivateKey` to sign the proposal directly.
#[async_trait::async_trait]
pub trait ProposalSigningBackend {
    /// A method to be called first to check if this backend can handle this proposal or not.
    async fn can_handle_proposal(
        &self,
        proposal: &(impl ProposalTrait + Sync + Send + 'static),
    ) -> crate::Result<bool>;
    /// Send the Unsigned Proposal to the backend to start handling the signing process.
    async fn handle_proposal(
        &self,
        proposal: &(impl ProposalTrait + Sync + Send + 'static),
        metrics: Arc<metric::Metrics>,
    ) -> crate::Result<()>;
}
