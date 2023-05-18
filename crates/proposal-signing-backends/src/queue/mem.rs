use parking_lot::RwLock;
use std::collections::BTreeMap;
use std::sync::Arc;

use webb::evm::ethers;

use super::{policy::*, ProposalHash, QueuedProposal};

/// An in-memory implementation of a proposals queue.
///
/// The `InMemoryProposalsQueue` struct stores proposals in a `BTreeMap` data structure, where each proposal is associated
/// with a unique identifier of type [`ethers::types::H256`] which is the hash of the proposal.
///
/// The `InMemoryProposalsQueue` struct is `Clone` and `Debug`, allowing for easy cloning and debugging of the queue.
/// It is designed to be cheap to clone, as it internally uses an `Arc` to share the underlying `BTreeMap` across multiple
/// clones, and a `RwLock` to ensure safe concurrent access to the map.
///
/// # Sharing the Queue Across Threads
///
/// The `InMemoryProposalsQueue` struct is designed to be shared across multiple threads. It internally uses an `Arc`
/// to share the underlying `BTreeMap` across multiple clones, ensuring safe concurrent access to the map.
///
/// # Copying the Queue
///
/// The `InMemoryProposalsQueue` struct is also cheap to copy, as it implements the `Clone` trait. It internally uses an `Arc`
/// to share the underlying `BTreeMap`, which means that cloning the queue only involves incrementing the reference count
/// of the `Arc` and cloning the `RwLock` handles, rather than performing a deep copy of the entire map.
///
#[derive(Clone, Debug)]
pub struct InMemoryProposalsQueue {
    proposals: Arc<RwLock<BTreeMap<ethers::types::H256, QueuedProposal>>>,
}

impl InMemoryProposalsQueue {
    /// Creates a new `InMemoryProposalsQueue` that will store the proposals
    /// in memory.
    pub fn new() -> Self {
        Self {
            proposals: Arc::new(RwLock::new(Default::default())),
        }
    }
}

impl super::ProposalsQueue for InMemoryProposalsQueue {
    type Proposal = QueuedProposal;

    #[tracing::instrument(
        skip_all,
        fields(
            queue = "in_memory",
            proposal = hex::encode(proposal.full_hash()),
        )
    )]
    fn enqueue<Policy: ProposalsQueuePolicy>(
        &self,
        proposal: Self::Proposal,
        policy: Policy,
    ) -> webb_relayer_utils::Result<()> {
        let accepted = policy.check(&proposal, self);
        tracing::debug!(accepted = ?accepted, "proposal check result");
        accepted?;
        self.proposals
            .write()
            .insert(proposal.full_hash().into(), proposal);
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(queue = "in_memory"))]
    fn dequeue<Policy: ProposalsQueuePolicy>(
        &self,
        policy: Policy,
    ) -> webb_relayer_utils::Result<Option<Self::Proposal>> {
        let rlock = self.proposals.read();
        let values = rlock
            .iter()
            .map(|(k, v)| (*k, v.clone()))
            .collect::<Vec<_>>();
        // Drop the read lock before checking the proposals
        // since the policy might need to acquire the lock
        drop(rlock);
        if values.is_empty() {
            tracing::debug!("no proposals to dequeue");
            return Ok(None);
        }
        tracing::debug!(len = values.len(), "checking proposals for dequeue",);

        for (k, proposal) in values {
            tracing::debug!(
                proposal = hex::encode(k.as_bytes()),
                "about to dequeue",
            );
            match policy.check(&proposal, self) {
                Ok(_) => {
                    // lock and remove the proposal
                    self.proposals.write().remove(&k);
                    tracing::debug!(
                        proposal = hex::encode(k.as_bytes()),
                        "proposal passed policy check before dequeue",
                    );
                    return Ok(Some(proposal));
                }
                Err(e) => {
                    tracing::debug!(
                        reason = %e,
                        "proposal failed policy check before dequeue",
                    );
                }
            }
        }
        Ok(None)
    }

    fn len(&self) -> webb_relayer_utils::Result<usize> {
        let len = self.proposals.read().len();
        Ok(len)
    }

    fn clear(&self) -> webb_relayer_utils::Result<()> {
        self.proposals.write().clear();
        Ok(())
    }

    fn remove(
        &self,
        hash: [u8; 32],
    ) -> webb_relayer_utils::Result<Option<Self::Proposal>> {
        let maybe_prop = self.proposals.write().remove(&hash.into());
        Ok(maybe_prop)
    }

    fn retain<F>(&self, mut f: F) -> webb_relayer_utils::Result<()>
    where
        F: FnMut(&Self::Proposal) -> bool,
    {
        self.proposals.write().retain(|_k, v| f(v));
        Ok(())
    }

    fn modify_in_place<F>(&self, f: F) -> webb_relayer_utils::Result<()>
    where
        F: FnMut(&mut Self::Proposal) -> webb_relayer_utils::Result<()>,
    {
        self.proposals.write().values_mut().try_for_each(f)
    }
}
