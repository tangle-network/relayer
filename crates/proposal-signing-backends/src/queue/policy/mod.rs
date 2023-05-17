mod nonce;
mod time;

pub use nonce::*;
pub use time::*;

/// The `ProposalsQueuePolicy` trait defines the behavior of a policy that is applied to proposals in a queue.
/// A policy determines whether a proposal should be accepted or rejected based on specific criteria.
pub trait ProposalsQueuePolicy {
    /// Checks whether a proposal should be accepted or rejected based on the policy's criteria.
    ///
    /// # Arguments
    ///
    /// * `proposal`: A reference to the proposal to be checked.
    /// * `queue`: A reference to the proposals queue that contains the proposal.
    ///
    /// # Errors
    ///
    /// This method may return an error if the proposal fails to meet the policy's criteria.
    ///
    /// # Examples
    ///
    /// Implementations of this method should perform the necessary checks on the proposal.
    /// If the proposal does not meet the policy's criteria, an error should be returned.
    ///
    /// ```rust
    /// use webb_relayer_utils::Result;
    ///
    /// struct MyPolicy;
    ///
    /// impl ProposalsQueuePolicy for MyPolicy {
    ///     fn check<Q: super::ProposalsQueue>(
    ///         &self,
    ///         proposal: &Q::Proposal,
    ///         queue: &Q,
    ///     ) -> Result<()> {
    ///         // Perform checks on the proposal
    ///         // ...
    ///
    ///         // If the proposal fails to meet the criteria, return an error
    ///         // return Err(webb_relayer_utils::Error::new("Proposal does not meet policy criteria"));
    ///
    ///         // If the proposal meets the criteria, return Ok(())
    ///         Ok(())
    ///     }
    /// }
    /// ```
    fn check<Q: super::ProposalsQueue>(
        &self,
        proposal: &Q::Proposal,
        queue: &Q,
    ) -> webb_relayer_utils::Result<()>;
}

#[impl_trait_for_tuples::impl_for_tuples(5)]
impl ProposalsQueuePolicy for TupleIdentifier {
    fn check<Q: super::ProposalsQueue>(
        &self,
        proposal: &Q::Proposal,
        queue: &Q,
    ) -> webb_relayer_utils::Result<()> {
        for_tuples!( #( TupleIdentifier.check(proposal, queue)?; )* );
        Ok(())
    }
}
