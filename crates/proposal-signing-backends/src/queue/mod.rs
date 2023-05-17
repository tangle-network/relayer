use std::sync::{atomic, Arc};

use webb::evm::ethers;
use webb_proposals::{ProposalHeader, ProposalTrait};

/// A module for in-memory Proposals Queue.
mod mem;
/// A module for Proposals Polices.
pub mod policy;

/// A Proposal Queue is a simple Queue that holds the proposals that are going to be
/// signed or already signed and need to be sent to the target system to be executed.
pub trait ProposalsQueue {
    /// The policy type associated with the Proposal Queue.
    ///
    /// The [`policy::ProposalsQueuePolicy`] trait is implemented for tuples of up to 5 policies.
    /// These policies are chained together and applied sequentially.
    /// If any of the policies in the chain rejects the proposal, the enqueue operation fails and returns an error.
    type Policy: policy::ProposalsQueuePolicy;

    /// The proposal type associated with the Proposal Queue.
    type Proposal: ProposalTrait + ProposalMetadata;

    /// Enqueues a proposal into the queue with the specified policy.
    ///
    /// The proposal is added to the queue, and the provided policy is applied to determine if the proposal should be accepted.
    /// If the policy chain rejects the proposal, the enqueue operation fails and returns an error.
    ///
    /// # Arguments
    ///
    /// * `proposal` - A reference to the proposal to be enqueued.
    /// * `policy` - The policy to be applied to the proposal. The policy determines if the proposal should be accepted.
    ///
    /// # Returns
    ///
    /// Returns a `Result` indicating whether the enqueue operation was successful or not.
    /// If the enqueue operation fails, an error is returned. Possible errors include:
    /// - `EnqueueQueueError` if there is an issue during the enqueue operation, such as a policy rejection.
    ///
    fn enqueue(
        &self,
        proposal: Self::Proposal,
        policy: Self::Policy,
    ) -> webb_relayer_utils::Result<()>;

    /// Dequeues a proposal from the queue with the specified policy.
    ///
    /// The dequeue operation retrieves the next proposal from the queue that satisfies the provided policy.
    /// The policy is applied sequentially to the proposals in the queue until a matching proposal is found.
    ///
    /// # Arguments
    ///
    /// * `policy` - The policy to be applied in the dequeue operation. The policy determines if a proposal should be dequeued.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing an optional proposal.
    /// - If a proposal satisfying the policy is found, it is returned as `Some(proposal)`.
    /// - If no matching proposal is found, `None` is returned.
    /// If the dequeue operation fails, an error is returned. Possible errors include:
    /// - `DequeueQueueError` if there is an issue during the dequeue operation.
    ///
    fn dequeue(
        &self,
        policy: Self::Policy,
    ) -> webb_relayer_utils::Result<Option<Self::Proposal>>;

    /// Returns the length of the proposal queue.
    ///
    /// The length of the queue is the total number of proposals currently stored in it.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the length of the queue.
    fn len(&self) -> webb_relayer_utils::Result<usize>;
    /// Returns `true` if the proposal queue is empty.
    ///
    /// # Returns
    /// Returns a `Result` containing a boolean value.
    fn is_empty(&self) -> webb_relayer_utils::Result<bool> {
        Ok(self.len()? == 0)
    }
    /// Clears the proposal queue, removing all proposals.
    ///
    /// This operation removes all proposals from the queue, resulting in an empty queue.
    ///
    /// # Returns
    ///
    /// Returns a `Result` indicating whether the clear operation was successful or not.
    /// If the clear operation fails, an error is returned. Possible errors include:
    /// - `ClearQueueError` if there is an issue during the clear operation.
    ///
    fn clear(&self) -> webb_relayer_utils::Result<()>;

    /// Removes a proposal from the queue based on its hash.
    ///
    /// The proposal with the specified hash is removed from the queue.
    /// If no proposal with the given hash is found, `None` is returned.
    ///
    /// # Arguments
    ///
    /// * `hash` - The hash of the proposal to be removed from the queue.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing an optional proposal.
    /// - If a proposal with the specified hash is found, it is returned as `Some(proposal)`.
    /// - If no proposal with the given hash is found, `None` is returned.
    /// If the remove operation fails, an error is returned. Possible errors include:
    /// - `RemoveQueueError` if there is an issue during the remove operation.
    ///
    fn remove(
        &self,
        hash: [u8; 32],
    ) -> webb_relayer_utils::Result<Option<Self::Proposal>>;

    /// Retains only the proposals in the queue that satisfy the given predicate.
    ///
    /// This method removes all proposals from the queue that do not satisfy the provided predicate function.
    /// The predicate function takes a reference to a proposal, and returns `true` if the proposal should be retained,
    /// or `false` if it should be removed from the queue.
    ///
    /// # Arguments
    ///
    /// * `f` - A mutable closure or function that determines whether a proposal should be retained or removed.
    ///         The closure takes a reference to a proposal, and returns a boolean value.
    ///         - If the closure returns `true`, the proposal is retained in the queue.
    ///         - If the closure returns `false`, the proposal is removed from the queue.
    ///
    /// # Returns
    ///
    /// Returns a `Result` indicating whether the retain operation was successful or not.
    /// If the retain operation fails, an error is returned. Possible errors include:
    /// - `RetainQueueError` if there is an issue during the retain operation.
    ///
    fn retain<F>(&self, f: F) -> webb_relayer_utils::Result<()>
    where
        F: FnMut(&Self::Proposal) -> bool;

    /// Modifies a proposal in place using the provided closure.
    ///
    /// This method allows modifying a proposal in the queue using a mutable closure.
    /// The closure takes a mutable reference to a proposal and performs the required modifications.
    ///
    /// # Arguments
    ///
    /// * `f` - A mutable closure or function that modifies the proposal in place.
    ///         The closure takes a mutable reference to the proposal and returns a `Result`.
    ///         - If the closure returns `Ok(())`, the modification is successful.
    ///         - If the closure returns `Err(_)`, the modification fails and an error is returned.
    ///
    /// # Returns
    ///
    /// Returns a `Result` indicating whether the modification operation was successful or not.
    /// If the modification operation fails, an error is returned. Possible errors include:
    /// - `ModifyQueueError` if there is an issue during the modification operation.
    ///
    fn modify_in_place<F>(&self, f: F) -> webb_relayer_utils::Result<()>
    where
        F: FnMut(&mut Self::Proposal) -> webb_relayer_utils::Result<()>;
}

/// Associated metadata for a queued proposal.
#[derive(Debug, Clone)]
pub struct QueuedProposalMetadata {
    /// The time at which the proposal was enqueued.
    /// the value is the number of secs since the UNIX epoch.
    queued_at: Arc<atomic::AtomicU64>,
    /// The time at which the proposal should be dequeued.
    /// The value is the number of secs since the UNIX epoch.
    ///
    /// Intially, this value is `Zero` and is set to the current time when the proposal is enqueued.
    should_be_dequeued_at: Arc<atomic::AtomicU64>,
}

impl QueuedProposalMetadata {
    pub fn should_be_dequeued_at(&self) -> Option<u64> {
        let should_be_dequeued_at =
            self.should_be_dequeued_at.load(atomic::Ordering::SeqCst);
        if should_be_dequeued_at == 0 {
            None
        } else {
            Some(should_be_dequeued_at)
        }
    }

    pub fn queued_at(&self) -> u64 {
        self.queued_at.load(atomic::Ordering::SeqCst)
    }

    pub fn set_should_be_dequeued_at(&self, should_be_dequeued_at: u64) {
        self.should_be_dequeued_at
            .store(should_be_dequeued_at, atomic::Ordering::SeqCst);
    }
}

impl Default for QueuedProposalMetadata {
    fn default() -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        Self {
            queued_at: Arc::new(atomic::AtomicU64::new(now)),
            should_be_dequeued_at: Arc::new(atomic::AtomicU64::default()),
        }
    }
}

/// A Small Wrapper around a proposal that adds some metadata.
#[derive(Debug)]
pub struct QueuedProposal<P> {
    inner: P,
    metadata: QueuedProposalMetadata,
}

impl<P: Clone> Clone for QueuedProposal<P> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            metadata: self.metadata.clone(),
        }
    }
}

impl<P: ProposalTrait> QueuedProposal<P> {
    pub fn new(inner: P) -> Self {
        Self {
            inner,
            metadata: Default::default(),
        }
    }
}

/// A small trait to add more functionality to the [`ProposalTrait`].
/// This trait is used to calculate the hash of the proposal.
pub trait ProposalHash {
    /// Returns the hash of the full proposal.
    /// The full proposal is the proposal header concatenated with the proposal body.
    fn full_hash(&self) -> [u8; 32];
    /// Returns the hash of the proposal body.
    /// The proposal body is the proposal without the header.
    fn body_hash(&self) -> [u8; 32];
}

// *** Implementation of the `ProposalHash` trait for all types that implement `ProposalTrait` ***
impl<P> ProposalHash for P
where
    P: ProposalTrait,
{
    fn full_hash(&self) -> [u8; 32] {
        let full = self.to_vec();
        ethers::utils::keccak256(&full)
    }

    fn body_hash(&self) -> [u8; 32] {
        let full = self.to_vec();
        let body = &full[ProposalHeader::LENGTH..];
        ethers::utils::keccak256(body)
    }
}

impl<P> ProposalTrait for QueuedProposal<P>
where
    P: ProposalTrait,
{
    fn header(&self) -> webb_proposals::ProposalHeader {
        self.inner.header()
    }

    fn to_vec(&self) -> Vec<u8> {
        self.inner.to_vec()
    }
}

/// The `ProposalMetadata` trait is implemented by proposals that carry metadata.
/// Metadata is additional information that accompanies a proposal, like its nonce, creation time, etc.
///
/// Implementing `ProposalMetadata` requires implementing two methods:
///  - `metadata`: Returns a reference to the metadata of the proposal.
///
/// The metadata is stored as a `QueuedProposalMetadata` struct.
/// This metadata is intended to be used by the queueing policy to make decisions about the proposal,
/// for example, to apply a time delay or prioritize the proposal based on its nonce.
pub trait ProposalMetadata {
    /// Returns a reference to the metadata of the proposal.
    fn metadata(&self) -> &QueuedProposalMetadata;
}

impl<P> ProposalMetadata for QueuedProposal<P>
where
    P: ProposalTrait,
{
    fn metadata(&self) -> &QueuedProposalMetadata {
        &self.metadata
    }
}

pub struct DummyProposal;

impl ProposalTrait for DummyProposal {
    fn header(&self) -> webb_proposals::ProposalHeader {
        todo!()
    }

    fn to_vec(&self) -> Vec<u8> {
        todo!()
    }
}

impl ProposalMetadata for DummyProposal {
    fn metadata(&self) -> &QueuedProposalMetadata {
        todo!()
    }
}

impl ProposalsQueue for () {
    type Policy = ();

    type Proposal = DummyProposal;

    fn enqueue(
        &self,
        proposal: Self::Proposal,
        policy: Self::Policy,
    ) -> webb_relayer_utils::Result<()> {
        todo!()
    }

    fn dequeue(
        &self,
        policy: Self::Policy,
    ) -> webb_relayer_utils::Result<Option<Self::Proposal>> {
        todo!()
    }

    fn len(&self) -> webb_relayer_utils::Result<usize> {
        todo!()
    }

    fn clear(&self) -> webb_relayer_utils::Result<()> {
        todo!()
    }

    fn remove(
        &self,
        hash: [u8; 32],
    ) -> webb_relayer_utils::Result<Option<Self::Proposal>> {
        todo!()
    }

    fn retain<F>(&self, f: F) -> webb_relayer_utils::Result<()>
    where
        F: FnMut(&Self::Proposal) -> bool,
    {
        todo!()
    }

    fn modify_in_place<F>(&self, f: F) -> webb_relayer_utils::Result<()>
    where
        F: FnMut(&mut Self::Proposal) -> webb_relayer_utils::Result<()>,
    {
        todo!()
    }
}

#[cfg(test)]
pub(crate) mod test_utils {
    use super::*;

    pub fn setup_tracing() -> tracing::subscriber::DefaultGuard {
        // Setup tracing for tests
        let s = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::builder()
                    .with_default_directive(
                        tracing_subscriber::filter::LevelFilter::DEBUG.into(),
                    )
                    .from_env_lossy(),
            )
            .with_test_writer()
            .pretty()
            .finish();
        tracing::subscriber::set_default(s)
    }

    pub fn mock_target_system(
        address: ethers::types::Address,
    ) -> webb_proposals::TargetSystem {
        webb_proposals::TargetSystem::ContractAddress(address.to_fixed_bytes())
    }

    pub fn mock_typed_chain_id(chain_id: u32) -> webb_proposals::TypedChainId {
        webb_proposals::TypedChainId::Evm(chain_id)
    }

    pub fn mock_resourc_id(
        target_system: webb_proposals::TargetSystem,
        typed_chain_id: webb_proposals::TypedChainId,
    ) -> webb_proposals::ResourceId {
        webb_proposals::ResourceId::new(target_system, typed_chain_id)
    }

    pub fn mock_proposal_header(
        target_resource_id: webb_proposals::ResourceId,
        nonce: u32,
    ) -> webb_proposals::ProposalHeader {
        let fnsig = [0x42u8; 4];
        webb_proposals::ProposalHeader::new(
            target_resource_id,
            fnsig.into(),
            nonce.into(),
        )
    }

    pub fn mock_evm_anchor_update_proposal(
        header: webb_proposals::ProposalHeader,
        src_resourc_id: webb_proposals::ResourceId,
    ) -> QueuedProposal<webb_proposals::evm::AnchorUpdateProposal> {
        let root = ethers::types::H256::random();
        let proposal = webb_proposals::evm::AnchorUpdateProposal::new(
            header,
            root.to_fixed_bytes(),
            src_resourc_id,
        );
        QueuedProposal::new(proposal)
    }
}
