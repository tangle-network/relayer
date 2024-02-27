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

use webb_proposals::ProposalTrait;

use crate::queue::ProposalsQueue;
/// A policy that always checks if the nonce of a proposal is higher than existing proposals,
/// while also removing any proposals with a lower nonce.
///
/// ## Expected Behavior
/// - Finds any proposal that matches the same (function signature, resource id) as the one being checked
///   and has a higher nonce.
/// - If such a proposal exists, rejects the proposal.
/// - If no such proposal exists, accepts the proposal, then
/// - Removes any proposals that match the same (function signature, resource id) as the one being checked
///   and have a lower nonce.
///
/// This way, we ensure that the queue only contains proposals with the highest nonce for a given
/// (function signature, resource id).
///
/// ## Note
/// This policy is stateless, hence it is cheap to copy.
#[derive(Debug, Copy, Clone, Default)]
pub struct AlwaysHigherNoncePolicy;

impl super::ProposalPolicy for AlwaysHigherNoncePolicy {
    #[tracing::instrument(skip_all)]
    fn check<Q: ProposalsQueue>(
        &self,
        proposal: &Q::Proposal,
        queue: &Q,
    ) -> webb_relayer_utils::Result<()> {
        let header = proposal.header();
        let nonce = header.nonce();
        let r_id = header.resource_id();
        let funsig = header.function_signature();

        let accept = queue
            .find(|p| {
                let p_header = p.header();
                p_header.function_signature().eq(&funsig)
                    && p_header.resource_id().eq(&r_id)
                    && p_header.nonce() > nonce
            })?
            .is_none();

        tracing::trace!(
            accept,
            nonce = nonce.to_u32(),
            "should accept proposal"
        );

        if !accept {
            return Err(webb_relayer_utils::Error::Generic("Nonce is too low"));
        }

        queue.retain(|p| {
            let p_header = p.header();
            let same_funsig = p_header.function_signature().eq(&funsig);
            if !same_funsig {
                return true;
            }
            let same_r_id = p_header.resource_id().eq(&r_id);
            if !same_r_id {
                return true;
            }
            // now we know that the proposal is for the same resource and function
            // signature. We can now check if the nonce is higher.
            let should_keep = p_header.nonce() > nonce;
            tracing::trace!(
                should_keep,
                nonce = nonce.to_u32(),
                p_nonce = p_header.nonce().to_u32(),
                "should keep proposal"
            );
            should_keep
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use webb::evm::ethers;

    use super::*;
    use crate::queue::{mem::InMemoryProposalsQueue, test_utils::*};

    type TestQueue = InMemoryProposalsQueue;

    #[test]
    fn should_accept_proposals_with_higher_nonce() {
        let policy = AlwaysHigherNoncePolicy;
        let queue = TestQueue::new();

        let target_system = mock_target_system(ethers::types::Address::zero());
        let target_chain = mock_typed_chain_id(1);

        let src_system = mock_target_system(ethers::types::Address::zero());
        let src_chain = mock_typed_chain_id(42);

        let r_id = mock_resourc_id(target_system, target_chain);
        let src_r_id = mock_resourc_id(src_system, src_chain);

        let header = mock_proposal_header(r_id, 1);
        let proposal = mock_evm_anchor_update_proposal(header, src_r_id);
        assert!(
            queue.enqueue(proposal, policy).is_ok(),
            "should accept proposal"
        );
    }

    #[test]
    fn should_reject_proposals_with_lower_nonce() {
        let policy = AlwaysHigherNoncePolicy;
        let queue = TestQueue::new();
        let target_system = mock_target_system(ethers::types::Address::zero());
        let target_chain = mock_typed_chain_id(1);
        let src_system = mock_target_system(ethers::types::Address::zero());
        let src_chain = mock_typed_chain_id(42);
        let r_id = mock_resourc_id(target_system, target_chain);
        let src_r_id = mock_resourc_id(src_system, src_chain);

        let header = mock_proposal_header(r_id, 1);
        let proposal = mock_evm_anchor_update_proposal(header, src_r_id);
        queue.enqueue(proposal, policy).unwrap();

        let header = mock_proposal_header(r_id, 0);
        let proposal = mock_evm_anchor_update_proposal(header, src_r_id);
        assert!(
            queue.enqueue(proposal, policy).is_err(),
            "should reject proposal"
        );
    }

    #[test]
    fn should_remove_any_proposal_with_lower_nonce() {
        let policy = AlwaysHigherNoncePolicy;
        let queue = TestQueue::new();
        let target_system = mock_target_system(ethers::types::Address::zero());
        let target_chain = mock_typed_chain_id(1);
        let src_system = mock_target_system(ethers::types::Address::zero());
        let src_chain = mock_typed_chain_id(42);
        let r_id = mock_resourc_id(target_system, target_chain);
        let src_r_id = mock_resourc_id(src_system, src_chain);
        // fill the queue with proposals.
        for nonce in 1..=10 {
            let header = mock_proposal_header(r_id, nonce);
            let proposal = mock_evm_anchor_update_proposal(header, src_r_id);
            queue.enqueue(proposal, policy).unwrap();
        }
        // we clean up the queue by adding a proposal with a higher nonce.
        let len = queue.len().unwrap();
        assert_eq!(len, 1, "should have only one proposal");
        let prop = queue.dequeue(()).unwrap().unwrap();
        assert_eq!(
            prop.header().nonce().to_u32(),
            10,
            "should have the highest nonce"
        );
    }

    #[test]
    fn should_accept_proposals_with_equal_nonce() {
        let policy = AlwaysHigherNoncePolicy;
        let queue = TestQueue::new();
        let target_system = mock_target_system(ethers::types::Address::zero());
        let target_chain = mock_typed_chain_id(1);
        let src_system = mock_target_system(ethers::types::Address::zero());
        let src_chain = mock_typed_chain_id(42);
        let r_id = mock_resourc_id(target_system, target_chain);
        let src_r_id = mock_resourc_id(src_system, src_chain);
        let header = mock_proposal_header(r_id, 1);
        let proposal = mock_evm_anchor_update_proposal(header, src_r_id);
        assert!(
            queue.enqueue(proposal.clone(), policy).is_ok(),
            "should accept proposal with equal nonce"
        );
        let prop = queue.dequeue(()).unwrap().unwrap();
        assert_eq!(
            prop.header().nonce().to_u32(),
            1,
            "should have the same nonce"
        );
        assert_eq!(
            proposal.header().nonce().to_u32(),
            prop.header().nonce().to_u32(),
            "nonce should be equal"
        );
    }

    #[test]
    fn should_handle_queue_with_higher_nonce_proposals() {
        let _guard = setup_tracing();
        let policy = AlwaysHigherNoncePolicy;
        let queue = TestQueue::new();
        let target_system = mock_target_system(ethers::types::Address::zero());
        let target_chain = mock_typed_chain_id(1);
        let src_system = mock_target_system(ethers::types::Address::zero());
        let src_chain = mock_typed_chain_id(42);
        let r_id = mock_resourc_id(target_system, target_chain);
        let src_r_id = mock_resourc_id(src_system, src_chain);
        let n = 10;
        // Fill the queue with proposals.
        for nonce in 6..=n {
            let header = mock_proposal_header(r_id, nonce);
            let proposal = mock_evm_anchor_update_proposal(header, src_r_id);
            assert!(
                queue.enqueue(proposal.clone(), policy).is_ok(),
                "should accept proposal with higher nonce"
            );
        }
        let len = queue.len().unwrap();
        assert_eq!(
            len, 1,
            "len should be equal to one for queue with higher nonce proposals"
        );
        let prop = queue.dequeue(()).unwrap().unwrap();
        let nonce = prop.header().nonce().to_u32();
        assert!(nonce == n, "nonce should equal to n");
    }

    #[test]
    fn should_handle_concurrent_operations() {
        use std::thread;

        let _guard = setup_tracing();
        let policy = AlwaysHigherNoncePolicy;
        let queue = TestQueue::new();
        let target_system = mock_target_system(ethers::types::Address::zero());
        let target_chain = mock_typed_chain_id(1);
        let src_system = mock_target_system(ethers::types::Address::zero());
        let src_chain = mock_typed_chain_id(42);
        let r_id = mock_resourc_id(target_system, target_chain);
        let src_r_id = mock_resourc_id(src_system, src_chain);
        // Concurrently enqueue and dequeue proposals
        let enqueue_handle = {
            let queue = queue.clone();
            // spawn a new thread, propagating the dispatcher context
            let dispatch = tracing::dispatcher::Dispatch::default();
            thread::spawn(move || {
                let _guard = tracing::dispatcher::set_default(&dispatch);
                for nonce in 1..=5 {
                    let header = mock_proposal_header(r_id, nonce);
                    let proposal =
                        mock_evm_anchor_update_proposal(header, src_r_id);
                    queue.enqueue(proposal, policy).unwrap();
                }
            })
        };

        let dequeue_handle = {
            let queue = queue.clone();
            // spawn a new thread, propagating the dispatcher context
            let dispatch = tracing::dispatcher::Dispatch::default();
            thread::spawn(move || {
                let _guard = tracing::dispatcher::set_default(&dispatch);
                for _ in 1..=5 {
                    // To make sure that the other thread has a chance to enqueue a proposal
                    std::thread::sleep(Duration::from_millis(50));
                    let _ = queue.dequeue(()).unwrap();
                }
            })
        };

        enqueue_handle.join().unwrap();
        dequeue_handle.join().unwrap();

        let len = queue.len().unwrap();
        assert_eq!(
            len, 0,
            "len should be 0 after concurrent enqueue and dequeue"
        );
    }
}
