use std::sync::Arc;

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use webb::evm::ethers::{types::transaction::eip2718::TypedTransaction, utils};
use webb_relayer_utils::static_tx_payload::TypeErasedStaticTxPayload;

/// A trait for retrieving queue keys
pub trait QueueKey {
    /// The Queue name, used as a prefix for the keys.
    fn queue_name(&self) -> String;
    /// an _optional_ different key for the same value stored in the queue.
    ///
    /// This useful for the case when you want to have a direct access to a specific key in the queue.
    /// For example, if you want to remove an item from the queue, you can use this key to directly
    /// remove it from the queue.
    fn item_key(&self) -> Option<[u8; 64]>;
}

/// A Queue item that wraps the inner item and maintains its state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QueueItem<T> {
    /// The inner value wrapped by the Queue Item.
    inner: T,
    /// The current state of the item in the queue.
    state: QueueItemState,
    /// The time when the item was enqueued.
    enqueued_at: u128,
    /// Time to live
    ttl: u128,
}

impl<T> QueueItem<T> {
    /// Creates a new QueueItem with the provided inner value.
    pub fn new(inner: T) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("Time went backwards");

        Self {
            inner,
            state: Default::default(),
            enqueued_at: now.as_millis(),
            ttl: 3 * 60 * 60 * 1000, // 3 hours
        }
    }
    /// Returns the state of the QueueItem.
    pub fn state(&self) -> QueueItemState {
        self.state.clone()
    }

    /// Unwraps the QueueItem and returns the inner value.
    pub fn inner(self) -> T {
        self.inner
    }
    /// set item state.
    pub fn set_state(&mut self, state: QueueItemState) {
        self.state = state;
    }
    /// Checks if item has been expired.
    pub fn is_expired(&self) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("SystemTime before UNIX EPOCH!");

        let current_time = now.as_millis();
        let expiration_time = self.enqueued_at + self.ttl;
        current_time > expiration_time
    }
}

/// The status of the item in the queue.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub enum QueueItemState {
    /// The current item is pending and waiting in the queue to be dequeued and processed.
    #[default]
    Pending,
    /// The item is being processed.
    Processing {
        /// A meaningful step for the current item state.
        step: String,
        /// A meaningful progress percentage for the current item state (0 to 1).
        progress: Option<f32>,
    },
    /// The item failed to be processed.
    Failed {
        /// The error message.
        reason: String,
    },
    /// The item was successfully processed.
    Processed,
}

/// A Queue Store is a simple trait that help storing items in a queue.
/// The queue is a FIFO queue, that can be used to store anything that can be serialized.
///
/// There is a simple API to get the items from the queue, from a background task for example.
pub trait QueueStore<Item>
where
    Item: Serialize + DeserializeOwned + Clone,
{
    /// The type of the queue key.
    type Key: QueueKey;
    /// Insert an item into the queue.
    fn enqueue_item(
        &self,
        key: Self::Key,
        item: QueueItem<Item>,
    ) -> crate::Result<()>;
    /// Get an item from the queue, and removes it.
    fn dequeue_item(
        &self,
        key: Self::Key,
    ) -> crate::Result<Option<QueueItem<Item>>>;
    /// Get an item from the queue, without removing it.
    fn peek_item(
        &self,
        key: Self::Key,
    ) -> crate::Result<Option<QueueItem<Item>>>;
    /// Check if the item is in the queue.
    fn has_item(&self, key: Self::Key) -> crate::Result<bool>;
    /// Remove an item from the queue.
    fn remove_item(
        &self,
        key: Self::Key,
    ) -> crate::Result<Option<QueueItem<Item>>>;
    /// Updates an item in the queue in-place.
    ///
    /// To update an item in the queue, you MUST provide the [`QueueKey::item_key`].
    /// Otherwise, the implementation does not know which item to update.
    fn update_item<F>(&self, key: Self::Key, f: F) -> crate::Result<bool>
    where
        F: FnMut(&mut QueueItem<Item>) -> crate::Result<()>;

    /// Shift item to the end of the queue.
    fn shift_item_to_end<F>(&self, key: Self::Key, f: F) -> crate::Result<bool>
    where
        F: FnMut(&mut QueueItem<Item>) -> crate::Result<()>;
}

impl<S, T> QueueStore<T> for Arc<S>
where
    S: QueueStore<T>,
    T: Serialize + DeserializeOwned + Clone,
{
    type Key = S::Key;

    fn enqueue_item(
        &self,
        key: Self::Key,
        item: QueueItem<T>,
    ) -> crate::Result<()> {
        S::enqueue_item(self, key, item)
    }

    fn dequeue_item(
        &self,
        key: Self::Key,
    ) -> crate::Result<Option<QueueItem<T>>> {
        S::dequeue_item(self, key)
    }

    fn peek_item(&self, key: Self::Key) -> crate::Result<Option<QueueItem<T>>> {
        S::peek_item(self, key)
    }

    fn has_item(&self, key: Self::Key) -> crate::Result<bool> {
        S::has_item(self, key)
    }

    fn remove_item(
        &self,
        key: Self::Key,
    ) -> crate::Result<Option<QueueItem<T>>> {
        S::remove_item(self, key)
    }

    fn update_item<F>(&self, key: Self::Key, f: F) -> crate::Result<bool>
    where
        F: FnMut(&mut QueueItem<T>) -> crate::Result<()>,
    {
        S::update_item(self, key, f)
    }

    fn shift_item_to_end<F>(&self, key: Self::Key, f: F) -> crate::Result<bool>
    where
        F: FnMut(&mut QueueItem<T>) -> crate::Result<()>,
    {
        S::shift_item_to_end(self, key, f)
    }
}

/// Create unique key for queue item, which can we used to update and remove item from queue.
pub trait TransactionQueueItemKey {
    fn item_key(&self) -> [u8; 64];
}

impl TransactionQueueItemKey for TypedTransaction {
    fn item_key(&self) -> [u8; 64] {
        let data_hash = self.sighash().0;
        let mut key = [0u8; 64];
        let prefix = b"evm_transaction_queue_item_key__";
        key[0..32].copy_from_slice(prefix);
        key[32..].copy_from_slice(&data_hash);
        key
    }
}

impl TransactionQueueItemKey for TypeErasedStaticTxPayload {
    fn item_key(&self) -> [u8; 64] {
        let data_hash = utils::keccak256(self.clone().call_data);
        let mut key = [0u8; 64];
        let prefix = b"substrate_transaction_item_key__";
        key[0..32].copy_from_slice(prefix);
        key[32..].copy_from_slice(&data_hash);
        key
    }
}
