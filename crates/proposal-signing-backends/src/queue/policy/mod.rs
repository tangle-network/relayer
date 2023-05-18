mod nonce;
mod time;

pub use nonce::*;
pub use time::*;

/// The `ProposalPolicy` trait defines the behavior of a policy that is applied to proposal in a queue.
/// A policy determines whether a proposal should be accepted or rejected based on specific criteria.
pub trait ProposalPolicy {
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
    fn check<Q: super::ProposalsQueue>(
        &self,
        proposal: &Q::Proposal,
        queue: &Q,
    ) -> webb_relayer_utils::Result<()>;
}

#[impl_trait_for_tuples::impl_for_tuples(5)]
impl ProposalPolicy for TupleIdentifier {
    fn check<Q: super::ProposalsQueue>(
        &self,
        proposal: &Q::Proposal,
        queue: &Q,
    ) -> webb_relayer_utils::Result<()> {
        for_tuples!( #( TupleIdentifier.check(proposal, queue)?; )* );
        Ok(())
    }
}
