use std::{
    ops::Add,
    sync::{atomic, Arc, Mutex},
    time::Duration,
};

use crate::queue::{ProposalHash, ProposalMetadata, ProposalsQueue};

/// Initial delay in seconds
const INITIAL_DELAY: u64 = 30;
/// Minimum delay in seconds
const MIN_DELAY: u64 = 10;
/// Maximum delay in seconds
const MAX_DELAY: u64 = 300;
/// Sliding window size
const WINDOW_SIZE: usize = 5;

/// A policy for introducing time delays based on a sliding window average.
///
/// The `TimeDelayPolicy` adjusts the current delay based on the average delay of the recent window of delays.
/// It provides control over the initial delay, minimum delay, maximum delay, and sliding window size.
///
/// # Example
///
/// ```rust,no_run
/// # use webb_proposal_signing_backends::queue::policy::TimeDelayPolicy;
/// use std::time::Duration;
///
/// // Create a TimeDelayPolicy with custom configuration
/// let policy = TimeDelayPolicy::builder()
///     .initial_delay(10)
///     .min_delay(5)
///     .max_delay(30)
///     .window_size(5)
///     .build();
///
/// // Get the current delay
/// let current_delay = policy.delay();
/// assert_eq!(current_delay, Duration::from_secs(10));
///```
///
#[derive(Debug, Clone, typed_builder::TypedBuilder)]
pub struct TimeDelayPolicy {
    /// Initial delay in seconds
    #[builder(default = INITIAL_DELAY)]
    initial_delay: u64,
    /// Minimum delay in seconds
    #[builder(default = MIN_DELAY)]
    min_delay: u64,
    /// Maximum delay in seconds
    #[builder(default = MAX_DELAY)]
    max_delay: u64,
    /// Sliding window size
    #[builder(default = WINDOW_SIZE)]
    window_size: usize,
    /// Sliding window of delays
    /// The sliding window is used to calculate the average delay
    /// The average delay is used to adjust the current delay
    /// Keeps the [`WINDOW_SIZE`] most recent delays
    #[builder(setter(skip), default = Arc::new(Mutex::new(vec![initial_delay; window_size])))]
    delays: Arc<Mutex<Vec<u64>>>,
    /// Current delay in seconds
    #[builder(setter(skip), default = Arc::new(atomic::AtomicU64::new(initial_delay)))]
    current_delay: Arc<atomic::AtomicU64>,
}

impl TimeDelayPolicy {
    /// Updates the current delay based on the average delay
    /// returns true if the delay was updated
    /// otherwise returns false, indecating that the delay was not updated
    #[tracing::instrument(skip(self))]
    pub fn update_delay(
        &self,
        num_proposals: usize,
    ) -> webb_relayer_utils::Result<bool> {
        // Add the current delay to the sliding window
        let mut lock = self.delays.lock().map_err(|_| {
            webb_relayer_utils::Error::Generic(
                "Failed to acquire lock on delays",
            )
        })?;

        // Add the current delay to the sliding window
        lock.push(self.current_delay.load(atomic::Ordering::Relaxed));
        // Remove the oldest delay from the sliding window
        lock.remove(0);

        // Calculate the average delay from the sliding window
        let sum: u64 = lock.iter().sum();
        let avg_delay = sum / lock.len() as u64;
        // Adjust the current delay based on the average and the number of proposals
        let adjusted_delay = if num_proposals > 0 {
            let scaling_factor = num_proposals as u64;
            let adjusted = avg_delay * scaling_factor;
            adjusted.min(self.max_delay).max(self.min_delay)
        } else {
            self.initial_delay
        };
        tracing::debug!(
            adjusted_delay,
            avg_delay,
            self.initial_delay,
            self.min_delay,
            self.max_delay,
            current_window_size = lock.len(),
            self.window_size,
            "Calculated Adjusted delay"
        );

        let old_delay = self
            .current_delay
            .swap(adjusted_delay, atomic::Ordering::Relaxed);
        Ok(adjusted_delay != old_delay)
    }

    /// Returns the current delay as a [`Duration`]
    pub fn delay(&self) -> Duration {
        Duration::from_secs(self.current_delay.load(atomic::Ordering::Relaxed))
    }

    /// Returns the maximum delay as a [`Duration`]
    pub fn max_delay(&self) -> Duration {
        Duration::from_secs(self.max_delay)
    }

    /// Returns the minimum delay as a [`Duration`]
    pub fn min_delay(&self) -> Duration {
        Duration::from_secs(self.min_delay)
    }
}

impl super::ProposalsQueuePolicy for TimeDelayPolicy {
    #[tracing::instrument(
        skip_all
        fields(
            proposal_hash = hex::encode(proposal.full_hash()),
            proposal_queued_at = proposal.metadata().queued_at(),
        )
    )]
    fn check<Q: ProposalsQueue>(
        &self,
        proposal: &Q::Proposal,
        queue: &Q,
    ) -> webb_relayer_utils::Result<()> {
        let size = queue.len()?;
        let delay_changed = self.update_delay(size)?;
        let delay = self.delay().as_secs();
        tracing::debug!(delay_changed, delay, queue_size = size);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        let metadata = proposal.metadata();
        // check if the proposal should be dequeued
        let ret = match metadata.should_be_dequeued_at() {
            Some(v) if v <= now => {
                // this means we are trying to dequeue a proposal.
                // and the proposal should be dequeued
                tracing::debug!(
                    should_be_dequeued_at = v,
                    now,
                    diff_secs = now - v,
                    "Proposal should be dequeued. Dequeuing proposal",
                );
                Ok(())
            }
            Some(v) => {
                tracing::debug!(
                    should_be_dequeued_at = v,
                    wait_secs = v - now,
                    "Proposal should not be dequeued",
                );
                Err(webb_relayer_utils::Error::Generic(
                    "Proposal should not be dequeued",
                ))
            }
            None => {
                let queued_at = metadata.queued_at();
                let expected_to_be_dequeued_at = queued_at.add(delay);
                // this means we are trying to queue a proposal.
                // we set the should_be_dequeued_at value
                metadata.set_should_be_dequeued_at(expected_to_be_dequeued_at);
                tracing::debug!(
                    queued_at,
                    should_be_dequeued_at = expected_to_be_dequeued_at,
                    would_wait_secs = expected_to_be_dequeued_at - queued_at,
                    "Proposal should be queued",
                );
                Ok(())
            }
        };
        // if the delay changed, adjust all the proposals in the queue with the new delay
        if delay_changed {
            tracing::debug!(
                "Delay changed. Adjusting all proposals in the queue"
            );
            queue.modify_in_place(|p| {
                let metadata = p.metadata();
                let queued_at = metadata.queued_at();
                let expected_to_be_dequeued_at = queued_at.add(delay);
                let should_be_dequeued_at = metadata.should_be_dequeued_at();
                metadata.set_should_be_dequeued_at(expected_to_be_dequeued_at);
                tracing::debug!(
                    queued_at,
                    old_should_be_dequeued_at = should_be_dequeued_at,
                    new_should_be_dequeued_at = expected_to_be_dequeued_at,
                    would_wait_secs = expected_to_be_dequeued_at - queued_at,
                    "Adjusted proposal in the queue",
                );
                Ok(())
            })?;
        }
        ret
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use webb::evm::ethers;

    use crate::queue::{mem::InMemoryProposalsQueue, test_utils::*};

    type TestQueue = InMemoryProposalsQueue;

    #[test]
    fn test_default_configuration() {
        let policy = TimeDelayPolicy::builder().build();
        assert_eq!(policy.delay(), Duration::from_secs(INITIAL_DELAY));
    }

    #[test]
    fn test_custom_configuration() {
        let policy = TimeDelayPolicy::builder()
            .initial_delay(10)
            .min_delay(5)
            .max_delay(30)
            .window_size(5)
            .build();
        assert_eq!(policy.delay(), Duration::from_secs(10));
    }

    #[test]
    fn test_large_window_size() {
        let policy = TimeDelayPolicy::builder().window_size(10).build();

        for _ in 0..10 {
            policy.update_delay(1).unwrap();
        }
        assert_eq!(policy.delay(), Duration::from_secs(INITIAL_DELAY));
    }

    #[test]
    fn test_zero_minimum_maximum_delay() {
        let policy =
            TimeDelayPolicy::builder().min_delay(0).max_delay(0).build();
        assert_eq!(policy.delay(), Duration::from_secs(INITIAL_DELAY));

        let _ = policy.update_delay(2);
        assert_eq!(policy.delay(), Duration::from_secs(0));
    }

    #[test]
    fn test_minimum_maximum_delay() {
        let policy = TimeDelayPolicy::builder()
            .initial_delay(10)
            .min_delay(10)
            .max_delay(10)
            .build();
        assert_eq!(policy.delay(), Duration::from_secs(10));

        policy.update_delay(0).unwrap();
        assert_eq!(policy.delay(), Duration::from_secs(10));
        policy.update_delay(2).unwrap();
        assert_eq!(policy.delay(), Duration::from_secs(10));
    }

    #[test]
    fn test_update_delay_with_zero_queue_size() {
        let policy = TimeDelayPolicy::builder().build();
        let result = policy.update_delay(0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_update_delay_with_zero_scaling_factor() {
        let policy = TimeDelayPolicy::builder().build();
        let result = policy.update_delay(0);
        assert!(result.is_ok());
    }

    #[test]
    fn test_zero_initial_delay() {
        let policy = TimeDelayPolicy::builder().initial_delay(0).build();
        assert_eq!(policy.delay(), Duration::from_secs(0));
    }

    #[test]
    fn test_zero_maximum_delay() {
        let policy = TimeDelayPolicy::builder().max_delay(0).build();
        assert_eq!(policy.delay(), Duration::from_secs(INITIAL_DELAY));

        policy.update_delay(2).unwrap();
        assert_eq!(policy.delay(), Duration::from_secs(MIN_DELAY));
    }

    #[test]
    fn test_zero_minimum_delay() {
        let policy = TimeDelayPolicy::builder().min_delay(0).build();
        assert_eq!(policy.delay(), Duration::from_secs(INITIAL_DELAY));

        policy.update_delay(2).unwrap();
        assert_eq!(policy.delay(), Duration::from_secs(2 * INITIAL_DELAY));
    }

    #[test]
    fn should_dequeue_proposal_at_the_right_time() {
        let _guard = setup_tracing();
        let policy = TimeDelayPolicy::builder()
            .initial_delay(1)
            .min_delay(0)
            .max_delay(2)
            .build();
        assert_eq!(policy.delay(), Duration::from_secs(1));

        let queue = TestQueue::new();

        let proposal = queue.dequeue(policy.clone()).unwrap();
        assert!(proposal.is_none(), "No Proposals to dequeue");

        let target_system = mock_target_system(ethers::types::Address::zero());
        let target_chain = mock_typed_chain_id(1);

        let src_system = mock_target_system(ethers::types::Address::zero());
        let src_chain = mock_typed_chain_id(42);

        let r_id = mock_resourc_id(target_system, target_chain);
        let src_r_id = mock_resourc_id(src_system, src_chain);

        let header = mock_proposal_header(r_id, 1);
        let proposal = mock_evm_anchor_update_proposal(header, src_r_id);
        assert!(
            queue.enqueue(proposal, policy.clone()).is_ok(),
            "should accept proposal"
        );
        // wait until the proposal is expected to be dequeued
        std::thread::sleep(policy.delay());
        let proposal = queue.dequeue(policy).unwrap();
        assert!(proposal.is_some(), "should dequeue proposal");
    }

    #[test]
    fn should_increase_delay_as_we_see_more_proposals() {
        let _guard = setup_tracing();
        let policy = TimeDelayPolicy::builder()
            .initial_delay(1)
            .min_delay(1)
            .max_delay(3)
            .build();
        assert_eq!(policy.delay(), Duration::from_secs(1));

        let queue = TestQueue::new();

        let proposal = queue.dequeue(policy.clone()).unwrap();
        assert!(proposal.is_none(), "No Proposals to dequeue");

        let target_system = mock_target_system(ethers::types::Address::zero());
        let target_chain = mock_typed_chain_id(1);

        let src_system = mock_target_system(ethers::types::Address::zero());
        let src_chain = mock_typed_chain_id(42);

        let r_id = mock_resourc_id(target_system, target_chain);
        let src_r_id = mock_resourc_id(src_system, src_chain);
        for i in 1..=10 {
            let header = mock_proposal_header(r_id, i);
            let proposal = mock_evm_anchor_update_proposal(header, src_r_id);
            assert!(
                queue.enqueue(proposal, policy.clone()).is_ok(),
                "should accept proposal"
            );
        }
        // the delay should be larger than the initial delay
        // but smaller than the max delay
        assert!(
            policy.delay() >= Duration::from_secs(policy.initial_delay)
                && policy.delay() <= Duration::from_secs(policy.max_delay),
            "delay should be larger than the initial delay but smaller than the max delay"
        );
        // if we did try to dequeue, we should not get any proposals
        // since the delay is not yet over.
        let proposal = queue.dequeue(policy.clone()).unwrap();
        assert!(proposal.is_none(), "Cannot dequeue proposal yet");
        // wait for the delay to expire
        std::thread::sleep(policy.delay());
        // we should be able to dequeue a proposal now
        let proposal = queue.dequeue(policy).unwrap();
        assert!(proposal.is_some(), "should dequeue proposal");
    }
}
