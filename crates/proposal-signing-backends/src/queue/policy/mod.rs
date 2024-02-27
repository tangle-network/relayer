// Copyright (C) 2022-2024 Webb Technologies Inc.
//
// Tangle is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Tangle is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should receive a copy of the GNU General Public License
// If not, see <http://www.gnu.org/licenses/>.

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

impl<P> ProposalPolicy for Option<P>
where
    P: ProposalPolicy,
{
    fn check<Q: super::ProposalsQueue>(
        &self,
        proposal: &Q::Proposal,
        queue: &Q,
    ) -> webb_relayer_utils::Result<()> {
        match self {
            Some(policy) => policy.check(proposal, queue),
            None => Ok(()),
        }
    }
}
