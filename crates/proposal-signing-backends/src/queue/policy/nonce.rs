use std::sync::{atomic, Arc};

use webb_proposals::ProposalTrait;

use crate::queue::{ProposalHash, ProposalsQueue};

/// A policy to check the nonce of the proposal before adding it to the queue.
/// This policy checks that the nonce of the proposal is higher than the last nonce.
/// If the nonce is lower than the last nonce, the proposal will be rejected.
/// This policy also removes any proposals from the queue that have a nonce lower than the current nonce.
#[derive(Debug, Clone)]
pub struct AlwaysHigherNoncePolicy {
    nonce: Arc<atomic::AtomicU32>,
}

impl AlwaysHigherNoncePolicy {
    /// Create a new `AlwaysHigherNoncePolicy` with the given nonce.
    pub fn new(nonce: u32) -> Self {
        Self {
            nonce: Arc::new(atomic::AtomicU32::from(nonce)),
        }
    }

    /// Get the current nonce.
    pub fn current_nonce(&self) -> u32 {
        self.nonce.load(atomic::Ordering::SeqCst)
    }
}

impl super::ProposalsQueuePolicy for AlwaysHigherNoncePolicy {
    #[tracing::instrument(
        skip_all
        fields(
            proposal_hash = hex::encode(proposal.full_hash()),
            proposal_nonce = proposal.header().nonce().to_u32(),
            nonce = self.nonce.load(atomic::Ordering::SeqCst),
        )
    )]
    fn check<Q: ProposalsQueue>(
        &self,
        proposal: &Q::Proposal,
        queue: &Q,
    ) -> webb_relayer_utils::Result<()> {
        let header = proposal.header();
        let nonce = header.nonce().to_u32();
        if nonce < self.nonce.load(atomic::Ordering::SeqCst) {
            tracing::debug!("nonce is low to accept the proposal");
            Err(webb_relayer_utils::Error::Generic("nonce is too low"))
        } else {
            // update the last nonce
            self.nonce.store(nonce, atomic::Ordering::SeqCst);
            tracing::debug!("nonce is high enough to accept the proposal");
            // Look through the queue and remove any proposals that have a nonce lower than the current nonce
            queue.retain(|p| {
                let p_header = p.header();
                let p_nonce = p_header.nonce().to_u32();
                let should_keep = p_nonce >= nonce;
                if !should_keep {
                    tracing::debug!(
                        marked_proposal = hex::encode(p.full_hash()),
                        marked_proposal_nonce = p_nonce,
                        "marked proposal for removal because it has a lower nonce than the current nonce"
                    );
                }
                should_keep
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use webb::evm::ethers;

    use super::*;
    use crate::queue::{
        mem::InMemoryProposalsQueue, test_utils::*, ProposalHash,
    };

    type TestQueue = InMemoryProposalsQueue;

    #[test]
    fn should_accept_proposals_with_higher_nonce() {
        let policy = AlwaysHigherNoncePolicy::new(0);
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
        let policy = AlwaysHigherNoncePolicy::new(1);
        let queue = TestQueue::new();
        let target_system = mock_target_system(ethers::types::Address::zero());
        let target_chain = mock_typed_chain_id(1);
        let src_system = mock_target_system(ethers::types::Address::zero());
        let src_chain = mock_typed_chain_id(42);
        let r_id = mock_resourc_id(target_system, target_chain);
        let src_r_id = mock_resourc_id(src_system, src_chain);
        let header = mock_proposal_header(r_id, 0);
        let proposal = mock_evm_anchor_update_proposal(header, src_r_id);
        assert!(
            queue.enqueue(proposal, policy).is_err(),
            "should reject proposal"
        );
    }

    #[test]
    fn should_remove_any_proposal_with_lower_nonce() {
        let policy = AlwaysHigherNoncePolicy::new(0);
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
            queue.enqueue(proposal, policy.clone()).unwrap();
        }
        assert_eq!(
            policy.nonce.load(atomic::Ordering::SeqCst),
            10,
            "policy should have the highest nonce"
        );
        // we clean up the queue by adding a proposal with a higher nonce.
        let len = queue.len().unwrap();
        assert_eq!(len, 1, "should have only one proposal");
        let prop = queue.dequeue(policy).unwrap().unwrap();
        assert_eq!(
            prop.header().nonce().to_u32(),
            10,
            "should have the highest nonce"
        );
    }

    #[test]
    fn should_accept_proposals_with_equal_nonce() {
        let policy = AlwaysHigherNoncePolicy::new(1);
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
            queue.enqueue(proposal.clone(), policy.clone()).is_ok(),
            "should accept proposal with equal nonce"
        );
        let prop = queue.dequeue(policy).unwrap().unwrap();
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
    fn should_handle_empty_queue() {
        let policy = AlwaysHigherNoncePolicy::new(0);
        let queue = TestQueue::new();
        let target_system = mock_target_system(ethers::types::Address::zero());
        let target_chain = mock_typed_chain_id(1);
        let src_system = mock_target_system(ethers::types::Address::zero());
        let src_chain = mock_typed_chain_id(42);
        let r_id = mock_resourc_id(target_system, target_chain);
        let src_r_id = mock_resourc_id(src_system, src_chain);
        let header = mock_proposal_header(r_id, 0);
        let proposal = mock_evm_anchor_update_proposal(header, src_r_id);
        assert!(
            queue.dequeue(policy).unwrap().is_none(),
            "dequeue should return None for an empty queue"
        );
        assert!(
            queue.remove(proposal.full_hash()).unwrap().is_none(),
            "remove should return None for an empty queue"
        );
        assert_eq!(
            queue.len().unwrap(),
            0,
            "len should be 0 for an empty queue"
        );
        assert!(
            queue.modify_in_place(|_| Ok(())).is_ok(),
            "modify_in_place should succeed for an empty queue"
        );
    }

    #[test]
    fn should_handle_queue_with_higher_nonce_proposals() {
        let policy = AlwaysHigherNoncePolicy::new(5);
        let queue = TestQueue::new();
        let target_system = mock_target_system(ethers::types::Address::zero());
        let target_chain = mock_typed_chain_id(1);
        let src_system = mock_target_system(ethers::types::Address::zero());
        let src_chain = mock_typed_chain_id(42);
        let r_id = mock_resourc_id(target_system, target_chain);
        let src_r_id = mock_resourc_id(src_system, src_chain);
        // Fill the queue with proposals.
        for nonce in 6..=10 {
            let header = mock_proposal_header(r_id, nonce);
            let proposal = mock_evm_anchor_update_proposal(header, src_r_id);
            assert!(
                queue.enqueue(proposal.clone(), policy.clone()).is_ok(),
                "should accept proposal with higher nonce"
            );
        }
        assert_eq!(
            policy.nonce.load(atomic::Ordering::SeqCst),
            10,
            "policy should have the highest nonce"
        );
        let len = queue.len().unwrap();
        assert_eq!(
            len, 1,
            "len should be equal to one for queue with higher nonce proposals"
        );
        let prop = queue.dequeue(policy.clone()).unwrap().unwrap();
        let nonce = prop.header().nonce().to_u32();
        assert!(
            nonce == policy.nonce.load(atomic::Ordering::SeqCst),
            "nonce should be equak policy's nonce"
        );
    }

    #[test]
    fn should_handle_concurrent_operations() {
        use std::thread;

        let _guard = setup_tracing();
        let policy = AlwaysHigherNoncePolicy::new(0);
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
            let policy = policy.clone();
            // spawn a new thread, propagating the dispatcher context
            let dispatch = tracing::dispatcher::Dispatch::default();
            thread::spawn(move || {
                let _guard = tracing::dispatcher::set_default(&dispatch);
                for nonce in 1..=5 {
                    let header = mock_proposal_header(r_id, nonce);
                    let proposal =
                        mock_evm_anchor_update_proposal(header, src_r_id);
                    queue.enqueue(proposal, policy.clone()).unwrap();
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
                    let _ = queue.dequeue(policy.clone()).unwrap();
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
