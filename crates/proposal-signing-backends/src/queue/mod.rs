use std::sync::{atomic, Arc};

use tokio::sync::Mutex;
use webb::evm::ethers;
use webb_proposals::ProposalTrait;
use webb_relayer_utils::metric;

/// A module for in-memory Proposals Queue.
pub mod mem;
/// A module for Proposals Polices.
pub mod policy;

/// A Proposal Queue is a simple Queue that holds the proposals that are going to be
/// signed or already signed and need to be sent to the target system to be executed.
pub trait ProposalsQueue {
    /// The proposal type associated with the Proposal Queue.
    type Proposal: ProposalTrait + ProposalMetadata + Send + Sync + 'static;

    /// Enqueues a proposal into the queue with the specified policy.
    ///
    ///
    /// The [`policy::ProposalsQueuePolicy`] trait is implemented for tuples of up to 5 policies.
    /// These policies are chained together and applied sequentially.
    /// If any of the policies in the chain rejects the proposal, the enqueue operation fails and returns an error.
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
    fn enqueue<Policy: policy::ProposalPolicy>(
        &self,
        proposal: Self::Proposal,
        policy: Policy,
    ) -> webb_relayer_utils::Result<()>;

    /// Dequeues a proposal from the queue with the specified policy.
    ///
    /// The dequeue operation retrieves the next proposal from the queue that satisfies the provided policy.
    /// The policy is applied sequentially to the proposals in the queue until a matching proposal is found.
    /// The [`policy::ProposalsQueuePolicy`] trait is implemented for tuples of up to 5 policies.
    /// These policies are chained together and applied sequentially.
    /// If any of the policies in the chain rejects the proposal, the dequeue operation fails and returns an error.
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
    fn dequeue<Policy: policy::ProposalPolicy>(
        &self,
        policy: Policy,
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

    /// Finds and returns a proposal that satisfies the given predicate.
    /// The predicate function takes a reference to a proposal, and returns `true` if the proposal is a match,
    /// or `false` if it is not a match.
    /// The first proposal that satisfies the predicate is returned.
    /// If no matching proposal is found, `None` is returned.
    fn find<F>(
        &self,
        f: F,
    ) -> webb_relayer_utils::Result<Option<Self::Proposal>>
    where
        F: FnMut(&Self::Proposal) -> bool;
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
    /// Get the time at which the proposal should be dequeued.
    /// The value is the number of secs since the UNIX epoch.
    pub fn should_be_dequeued_at(&self) -> Option<u64> {
        let should_be_dequeued_at =
            self.should_be_dequeued_at.load(atomic::Ordering::SeqCst);
        if should_be_dequeued_at == 0 {
            None
        } else {
            Some(should_be_dequeued_at)
        }
    }

    /// Get the time at which the proposal was enqueued.
    /// The value is the number of secs since the UNIX epoch.
    pub fn queued_at(&self) -> u64 {
        self.queued_at.load(atomic::Ordering::SeqCst)
    }

    /// Set the time at which the proposal should be dequeued.
    /// The value is the number of secs since the UNIX epoch.
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
#[derive(Clone)]
pub struct QueuedAnchorUpdateProposal {
    inner: Arc<dyn ProposalTrait + Sync + Send>,
    metadata: QueuedProposalMetadata,
}

impl std::fmt::Debug for QueuedAnchorUpdateProposal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QueuedProposal")
            .field("inner", &hex::encode(self.inner.to_vec()))
            .field("metadata", &self.metadata)
            .finish()
    }
}

impl QueuedAnchorUpdateProposal {
    /// Creates a new `QueuedAnchorUpdateProposal` from a proposal.
    /// This abstracts away the metadata creation.
    pub fn new<P>(inner: P) -> Self
    where
        P: ProposalTrait + Send + Sync + 'static,
    {
        Self {
            inner: Arc::new(inner),
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
}

// *** Implementation of the `ProposalHash` trait for all types that implement `ProposalTrait` ***
impl<P> ProposalHash for P
where
    P: ProposalTrait,
{
    fn full_hash(&self) -> [u8; 32] {
        ethers::utils::keccak256(self.to_vec())
    }
}

impl ProposalTrait for QueuedAnchorUpdateProposal {
    fn header(&self) -> webb_proposals::ProposalHeader {
        self.inner.header()
    }

    fn to_vec(&self) -> Vec<u8> {
        self.inner.to_vec()
    }

    fn function_sig(&self) -> webb_proposals::FunctionSignature {
        self.inner.header().function_signature()
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

impl ProposalMetadata for QueuedAnchorUpdateProposal {
    fn metadata(&self) -> &QueuedProposalMetadata {
        &self.metadata
    }
}

/// Runs the queue in a loop that it will try
/// to dequeue proposals and sends them to the signing backend.
///
/// This function will loop forever and should be run in a separate task.
/// it will never end unless the task is cancelled.
#[tracing::instrument(skip_all)]
pub async fn run<Queue, Policy, PSB>(
    queue: Queue,
    dequeue_policy: Policy,
    proposal_signing_backend: PSB,
    metrics: Arc<Mutex<metric::Metrics>>,
) where
    Queue: ProposalsQueue,
    Policy: policy::ProposalPolicy + Clone,
    PSB: super::ProposalSigningBackend,
{
    loop {
        let proposal = match queue.dequeue(dequeue_policy.clone()) {
            Ok(Some(proposal)) => proposal,
            Ok(None) => {
                tracing::trace!("No proposal to dequeue");
                // Sleep for a bit to avoid busy looping
                tokio::time::sleep(core::time::Duration::from_millis(1100))
                    .await;
                tokio::task::yield_now().await;
                continue;
            }
            Err(e) => {
                tracing::error!("Failed to dequeue proposal: {:?}", e);
                tokio::task::yield_now().await;
                continue;
            }
        };

        let result = crate::proposal_handler::handle_proposal(
            &proposal,
            &proposal_signing_backend,
            metrics.clone(),
        )
        .await;
        match result {
            Ok(_) => {
                tracing::trace!(
                    proposal = ?hex::encode(proposal.to_vec()),
                    "the proposal was successfully handled by the signing backend"
                );
            }
            Err(e) => {
                tracing::error!(
                    error = ?e,
                    proposal = ?hex::encode(proposal.to_vec()),
                    "failed to handle the proposal",
                );
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod test_utils {
    use super::*;

    pub fn setup_tracing() -> tracing::subscriber::DefaultGuard {
        // Setup tracing for tests
        let env_filter = tracing_subscriber::EnvFilter::builder()
            .with_default_directive(
                tracing_subscriber::filter::LevelFilter::DEBUG.into(),
            )
            .from_env_lossy();
        let s = tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_test_writer()
            .without_time()
            .with_target(false)
            .compact()
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
    ) -> QueuedAnchorUpdateProposal {
        let root = ethers::types::H256::random();
        let proposal = webb_proposals::evm::AnchorUpdateProposal::new(
            header,
            root.to_fixed_bytes(),
            src_resourc_id,
        );
        QueuedAnchorUpdateProposal::new(proposal)
    }

    pub fn mock_metrics() -> Arc<Mutex<metric::Metrics>> {
        Arc::new(Mutex::new(metric::Metrics::new().unwrap()))
    }

    #[derive(Clone, Debug, Default)]
    pub struct DummySigningBackend {
        pub handled_proposals_count: Arc<atomic::AtomicU32>,
    }

    #[async_trait::async_trait]
    impl crate::ProposalSigningBackend for DummySigningBackend {
        async fn can_handle_proposal(
            &self,
            _proposal: &(impl ProposalTrait + Sync + Send + 'static),
        ) -> webb_relayer_utils::Result<bool> {
            Ok(true)
        }
        async fn handle_proposal(
            &self,
            proposal: &(impl ProposalTrait + Sync + Send + 'static),
            _metrics: Arc<Mutex<metric::Metrics>>,
        ) -> webb_relayer_utils::Result<()> {
            tracing::debug!(
                proposal = ?hex::encode(proposal.to_vec()),
                "pretending to handle proposal",
            );
            self.handled_proposals_count
                .fetch_add(1, atomic::Ordering::SeqCst);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use rand::Rng;

    use super::test_utils::*;
    use super::*;

    #[tokio::test]
    async fn simulation() {
        let _guard = setup_tracing();
        let queue = mem::InMemoryProposalsQueue::new();
        let time_delay_policy = policy::TimeDelayPolicy::builder()
            .initial_delay(1)
            .min_delay(1)
            .max_delay(5)
            .build();
        let nonce_policy = policy::AlwaysHigherNoncePolicy;
        let enqueue_policy = (nonce_policy, time_delay_policy.clone());
        let dequeue_policy = time_delay_policy.clone();
        let signing_backend = DummySigningBackend::default();
        let metrics = mock_metrics();

        let handle = tokio::spawn(run(
            queue.clone(),
            dequeue_policy,
            signing_backend.clone(),
            metrics,
        ));

        let target_system = mock_target_system(ethers::types::Address::zero());
        let target_chain = mock_typed_chain_id(1);
        let src_system = mock_target_system(ethers::types::Address::zero());
        let src_chain = mock_typed_chain_id(42);
        let r_id = mock_resourc_id(target_system, target_chain);
        let src_r_id = mock_resourc_id(src_system, src_chain);
        // fill the queue with proposals.
        let n = 5;
        for nonce in 1..=n {
            let header = mock_proposal_header(r_id, nonce);
            let proposal = mock_evm_anchor_update_proposal(header, src_r_id);
            queue.enqueue(proposal, enqueue_policy.clone()).unwrap();
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        tokio::time::sleep(time_delay_policy.max_delay()).await;
        // all proposals should be handled by now.
        // the queue should be empty.
        assert!(queue.is_empty().unwrap(), "the queue should be empty");
        // we should only have handled one proposal.
        // the rest should be removed from the queue.
        assert_eq!(
            signing_backend
                .handled_proposals_count
                .load(atomic::Ordering::SeqCst),
            1,
            "we should only have handled one proposal",
        );
        handle.abort();
    }

    /// This tests simulate the case that we are running a proposal queue
    /// with nonce policy and time delay policy in the enqueue operation
    /// and time delay policy in the dequeue operation.
    #[ignore = "this test simulate a real usage hence the wait time is long"]
    #[tokio::test]
    async fn manual_simulation_case1() {
        let _guard = setup_tracing();
        let queue = mem::InMemoryProposalsQueue::new();
        let time_delay_policy = policy::TimeDelayPolicy::builder()
            .initial_delay(30)
            .min_delay(60)
            .max_delay(300)
            .build();
        let nonce_policy = policy::AlwaysHigherNoncePolicy;
        let enqueue_policy = (nonce_policy, time_delay_policy.clone());
        let dequeue_policy = time_delay_policy.clone();
        let signing_backend = DummySigningBackend::default();
        let metrics = mock_metrics();

        let handle = tokio::spawn(run(
            queue.clone(),
            dequeue_policy,
            signing_backend.clone(),
            metrics,
        ));

        let target_system = mock_target_system(ethers::types::Address::zero());
        let target_chain = mock_typed_chain_id(1);
        let r_id = mock_resourc_id(target_system, target_chain);

        let src_system = mock_target_system(ethers::types::Address::zero());
        let src_chain = mock_typed_chain_id(42);
        let src_r_id = mock_resourc_id(src_system, src_chain);

        // fill the queue with proposals.
        let n = 5;
        for nonce in 1..=n {
            let header = mock_proposal_header(r_id, nonce);
            let proposal = mock_evm_anchor_update_proposal(header, src_r_id);
            queue.enqueue(proposal, enqueue_policy.clone()).unwrap();
            // Simulate that a time passed between each proposal.
            // The time will be randomly choosed.
            let delay = rand::thread_rng().gen_range(10_000..60_000);
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }

        tokio::time::sleep(time_delay_policy.delay()).await;
        // all proposals should be handled by now.
        // the queue should be empty.
        assert!(queue.is_empty().unwrap(), "the queue should be empty");
        // we should only have handled one proposal.
        // the rest should be removed from the queue.
        assert_eq!(
            signing_backend
                .handled_proposals_count
                .load(atomic::Ordering::SeqCst),
            1,
            "we should only have handled one proposal",
        );
        handle.abort();
    }

    /// This tests simulate the case that we are running a proposal queue
    /// with a time delay policy in the enqueue operation
    /// and time delay policy in the dequeue operation.
    ///
    /// This test case is more specific to test if someone can DDOS the queue
    /// by sending a lot of proposals which would increase the delay of the
    /// queue and hence the queue will never process any proposal.
    ///
    /// We simulate this by sending a lot of proposals with a small delay in between
    /// and we expect that the queue will process at least one proposal in a reasonable
    /// time.
    #[ignore = "this test simulate a real usage hence the wait time is long"]
    #[tokio::test]
    async fn manual_simulation_case2() {
        let _guard = setup_tracing();
        let queue = mem::InMemoryProposalsQueue::new();
        let time_delay_policy = policy::TimeDelayPolicy::builder()
            .initial_delay(30)
            .min_delay(60)
            .max_delay(90)
            .build();
        let enqueue_policy = time_delay_policy.clone();
        let dequeue_policy = time_delay_policy.clone();
        let signing_backend = DummySigningBackend::default();
        let metrics = mock_metrics();

        let handle = tokio::spawn(run(
            queue.clone(),
            dequeue_policy,
            signing_backend.clone(),
            metrics,
        ));

        let target_system = mock_target_system(ethers::types::Address::zero());
        let target_chain = mock_typed_chain_id(1);
        let r_id = mock_resourc_id(target_system, target_chain);

        let src_system = mock_target_system(ethers::types::Address::zero());
        let src_chain = mock_typed_chain_id(42);
        let src_r_id = mock_resourc_id(src_system, src_chain);

        let queue2 = queue.clone();
        let handle2 = tokio::spawn(async move {
            let mut nonce = 1;
            // fill the queue with proposals.
            loop {
                let header = mock_proposal_header(r_id, nonce);
                let proposal =
                    mock_evm_anchor_update_proposal(header, src_r_id);
                queue2.enqueue(proposal, enqueue_policy.clone()).unwrap();
                // Simulate that a time passed between each proposal.
                // The time will be randomly choosed.
                let delay = rand::thread_rng().gen_range(5_000..12_000);
                tokio::time::sleep(Duration::from_millis(delay)).await;
                nonce += 1;
            }
        });

        // wait for the queue to process at least one proposal.
        tokio::time::sleep(time_delay_policy.max_delay().mul_f64(1.5)).await;
        // we should only have handled at least one proposal.
        assert!(
            signing_backend
                .handled_proposals_count
                .load(atomic::Ordering::SeqCst)
                .ge(&1),
            "we should have at least handled one proposal",
        );
        handle.abort();
        handle2.abort();
    }
}
