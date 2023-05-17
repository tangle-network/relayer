use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::RwLock;

use webb::evm::ethers;
use webb_proposals::ProposalTrait;

use super::policy::*;
use super::ProposalHash;
use super::QueuedProposal;

#[derive(Clone, Debug)]
pub struct InMemoryProposalsQueue<Proposal, Policy> {
    proposals:
        Arc<RwLock<BTreeMap<ethers::types::H256, QueuedProposal<Proposal>>>>,
    _marker: std::marker::PhantomData<Policy>,
}

impl<Proposal, Policy> InMemoryProposalsQueue<Proposal, Policy> {
    /// Creates a new `InMemoryProposalsQueue` that will store the proposals
    /// in memory.
    pub fn new() -> Self {
        Self {
            proposals: Arc::new(RwLock::new(Default::default())),
            _marker: std::marker::PhantomData,
        }
    }
}

impl<Proposal, Policy> super::ProposalsQueue
    for InMemoryProposalsQueue<Proposal, Policy>
where
    Proposal: ProposalTrait + Clone,
    Policy: ProposalsQueuePolicy,
{
    type Policy = Policy;

    type Proposal = QueuedProposal<Proposal>;

    #[tracing::instrument(
        skip_all,
        fields(
            queue = "in_memory",
            proposal = hex::encode(proposal.full_hash()),
        )
    )]
    fn enqueue(
        &self,
        proposal: Self::Proposal,
        policy: Self::Policy,
    ) -> webb_relayer_utils::Result<()> {
        let accepted = policy.check(&proposal, self);
        tracing::debug!(accepted = ?accepted, "proposal check result");
        accepted?;
        self.proposals
            .write()
            .map_err(|_| {
                webb_relayer_utils::Error::Generic(
                    "failed to acquire write lock on proposals",
                )
            })?
            .insert(proposal.full_hash().into(), proposal);
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(queue = "in_memory"))]
    fn dequeue(
        &self,
        policy: Self::Policy,
    ) -> webb_relayer_utils::Result<Option<Self::Proposal>> {
        let rlock = self.proposals.read().map_err(|_| {
            webb_relayer_utils::Error::Generic(
                "failed to acquire read lock on proposals",
            )
        })?;
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
                    let mut wlock = self.proposals.write().map_err(|_| {
                        webb_relayer_utils::Error::Generic(
                            "failed to acquire write lock on proposals",
                        )
                    })?;
                    wlock.remove(&k);
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
        let len = self
            .proposals
            .read()
            .map_err(|_| {
                webb_relayer_utils::Error::Generic(
                    "failed to acquire read lock on proposals",
                )
            })?
            .len();
        Ok(len)
    }

    fn clear(&self) -> webb_relayer_utils::Result<()> {
        self.proposals
            .write()
            .map_err(|_| {
                webb_relayer_utils::Error::Generic(
                    "failed to acquire write lock on proposals",
                )
            })?
            .clear();
        Ok(())
    }

    fn remove(
        &self,
        hash: [u8; 32],
    ) -> webb_relayer_utils::Result<Option<Self::Proposal>> {
        let maybe_prop = self
            .proposals
            .write()
            .map_err(|_| {
                webb_relayer_utils::Error::Generic(
                    "failed to acquire write lock on proposals",
                )
            })?
            .remove(&hash.into());
        Ok(maybe_prop)
    }

    fn retain<F>(&self, mut f: F) -> webb_relayer_utils::Result<()>
    where
        F: FnMut(&Self::Proposal) -> bool,
    {
        let mut write_lock = self.proposals.write().map_err(|_| {
            webb_relayer_utils::Error::Generic(
                "failed to acquire write lock on proposals",
            )
        })?;
        write_lock.retain(|_k, v| f(v));
        Ok(())
    }

    fn modify_in_place<F>(&self, mut f: F) -> webb_relayer_utils::Result<()>
    where
        F: FnMut(&mut Self::Proposal) -> webb_relayer_utils::Result<()>,
    {
        let mut write_lock = self.proposals.write().map_err(|_| {
            webb_relayer_utils::Error::Generic(
                "failed to acquire write lock on proposals",
            )
        })?;
        for (_k, v) in write_lock.iter_mut() {
            f(v)?;
        }
        Ok(())
    }
}
