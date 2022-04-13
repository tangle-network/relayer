#[doc(hidden)]
mod dkg;
#[doc(hidden)]
mod mocked;

pub use dkg::*;
pub use mocked::*;

#[async_trait::async_trait]
pub trait SigningBackend<P>
where
    P: Send + Sync + 'static,
{
    /// A method to be called first to check if this backend can handle this proposal or not.
    async fn can_handle_proposal(&self, proposal: &P) -> anyhow::Result<bool>;
    /// Send the Unsigned Proposal to the backend to start handling the signing process.
    async fn handle_proposal(&self, proposal: &P) -> anyhow::Result<()>;
}
