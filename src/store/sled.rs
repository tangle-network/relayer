use std::fmt::Debug;
use std::path::Path;
use webb::evm::ethers::core::types::transaction;
use webb::evm::ethers::types;

use super::{ContractKey, ProposalEntity};
use super::{HistoryStore, LeafCacheStore, ProposalStore, TxQueueStore};

#[derive(Clone)]
pub struct SledStore {
    db: sled::Db,
}

impl std::fmt::Debug for SledStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SledStore").finish()
    }
}

impl SledStore {
    pub fn open<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let db = sled::Config::new()
            .path(path)
            .temporary(cfg!(test))
            .use_compression(true)
            .compression_factor(18)
            .open()?;
        Ok(Self { db })
    }
}

impl HistoryStore for SledStore {
    #[tracing::instrument(skip(self))]
    fn set_last_block_number<K: Into<ContractKey> + Debug>(
        &self,
        key: K,
        block_number: types::U64,
    ) -> anyhow::Result<types::U64> {
        let tree = self.db.open_tree("last_block_numbers")?;
        let mut bytes = [0u8; std::mem::size_of::<types::U64>()];
        block_number.to_little_endian(&mut bytes);
        let key: ContractKey = key.into();
        let old = tree.insert(key.to_bytes(), &bytes)?;
        match old {
            Some(v) => Ok(types::U64::from_little_endian(&v)),
            None => Ok(block_number),
        }
    }

    #[tracing::instrument(skip(self))]
    fn get_last_block_number<K: Into<ContractKey> + Debug>(
        &self,
        key: K,
        default_block_number: types::U64,
    ) -> anyhow::Result<types::U64> {
        let tree = self.db.open_tree("last_block_numbers")?;
        let key: ContractKey = key.into();
        let val = tree.get(key.to_bytes())?;
        match val {
            Some(v) => Ok(types::U64::from_little_endian(&v)),
            None => Ok(default_block_number),
        }
    }
}

impl LeafCacheStore for SledStore {
    type Output = Vec<types::H256>;

    #[tracing::instrument(skip(self))]
    fn get_leaves<K: Into<ContractKey> + Debug>(
        &self,
        key: K,
    ) -> anyhow::Result<Self::Output> {
        let key: ContractKey = key.into();
        let tree = self
            .db
            .open_tree(format!("leaves/{}/{}", key.chain_id, key.address))?;
        let leaves = tree
            .iter()
            .values()
            .flatten()
            .map(|v| types::H256::from_slice(&v))
            .collect();
        Ok(leaves)
    }

    #[tracing::instrument(skip(self))]
    fn insert_leaves<K: Into<ContractKey> + Debug>(
        &self,
        key: K,
        leaves: &[(u32, types::H256)],
    ) -> anyhow::Result<()> {
        let key: ContractKey = key.into();

        let tree = self
            .db
            .open_tree(format!("leaves/{}/{}", key.chain_id, key.address))?;
        for (k, v) in leaves {
            tree.insert(k.to_le_bytes(), v.as_bytes())?;
        }
        Ok(())
    }

    fn get_last_deposit_block_number<K: Into<ContractKey> + Debug>(
        &self,
        key: K,
    ) -> anyhow::Result<types::U64> {
        let tree = self.db.open_tree("last_deposit_block_number")?;
        let key: ContractKey = key.into();
        let val = tree.get(key.to_bytes())?;
        match val {
            Some(v) => Ok(types::U64::from_little_endian(&v)),
            None => Ok(types::U64::from(0)),
        }
    }

    fn insert_last_deposit_block_number<K: Into<ContractKey> + Debug>(
        &self,
        key: K,
        block_number: types::U64,
    ) -> anyhow::Result<types::U64> {
        let tree = self.db.open_tree("last_deposit_block_number")?;
        let mut bytes = [0u8; std::mem::size_of::<types::U64>()];
        block_number.to_little_endian(&mut bytes);
        let key: ContractKey = key.into();
        let old = tree.insert(key.to_bytes(), &bytes)?;
        match old {
            Some(v) => Ok(types::U64::from_little_endian(&v)),
            None => Ok(block_number),
        }
    }
}

impl TxQueueStore for SledStore {
    #[tracing::instrument(
        skip_all,
        fields(
            chain_id = %chain_id,
            tx_key = %hex::encode(key)
        )
    )]
    fn enqueue_tx_with_key(
        &self,
        chain_id: types::U256,
        tx: transaction::eip2718::TypedTransaction,
        key: &[u8],
    ) -> anyhow::Result<()> {
        let tree = self.db.open_tree(format!("tx_queue_chain_{}", chain_id))?;
        let tx_bytes = serde_json::to_vec(&tx)?;
        // we do everything inside a single transaction
        // so everything happens atomically and if anything fails
        // we revert everything back to the old state.
        tree.transaction::<_, _, std::io::Error>(|db| {
            // get the last id of the queue.
            let last_tx_idx = match db.get("last_tx_idx")? {
                Some(v) => types::U64::from_big_endian(&v),
                None => types::U64::zero(),
            };
            // increment it.
            let next_idx = last_tx_idx + types::U64::one();
            let mut idx_bytes = [0u8; std::mem::size_of::<types::U64>()];
            next_idx.to_big_endian(&mut idx_bytes);
            // then save it.
            db.insert("last_tx_idx", &idx_bytes)?;
            db.insert("key_prefix", "tx")?;
            // we create a tx key like so
            // tx_key = 2 bytes prefix ("tx") + 8 bytes of the index.
            let mut tx_key = [0u8; 2 + std::mem::size_of::<types::U64>()];
            let prefix = db.get("key_prefix")?.unwrap_or_else(|| b"tx".into());
            tx_key[0..2].copy_from_slice(&prefix);
            tx_key[2..].copy_from_slice(&idx_bytes);
            // then we save it.
            db.insert(&tx_key, tx_bytes.as_slice())?;
            // also save the key where we can find it by special key.
            db.insert(key, &tx_key)?;
            let tx_hash = tx.sighash(chain_id.as_u64());
            tracing::trace!("enqueue transaction with txhash = {:?}", tx_hash);
            Ok(())
        })?;
        // flush the db to make sure we don't lose anything.
        self.db.flush()?;
        Ok(())
    }

    #[tracing::instrument(skip_all, fields(chain_id = %chain_id))]
    fn dequeue_tx(
        &self,
        chain_id: types::U256,
    ) -> anyhow::Result<Option<transaction::eip2718::TypedTransaction>> {
        let tree = self.db.open_tree(format!("tx_queue_chain_{}", chain_id))?;
        // now we create a lazy iterator that will scan
        // over all saved transactions in the queue
        // with the specific key prefix.
        let prefix = tree.get("key_prefix")?.unwrap_or_else(|| b"tx".into());
        let mut queue = tree.scan_prefix(prefix);
        let (key, value) = match queue.next() {
            Some(Ok(v)) => v,
            _ => {
                tracing::trace!("queue is empty ..");
                return Ok(None);
            }
        };
        let tx = serde_json::from_slice(&value)?;
        // now it is safe to remove it from the queue.
        tree.remove(key)?;
        // flush db
        self.db.flush()?;
        Ok(Some(tx))
    }

    fn peek_tx(
        &self,
        chain_id: types::U256,
    ) -> anyhow::Result<Option<transaction::eip2718::TypedTransaction>> {
        // this method, is similar to dequeue_tx, expect we don't
        // remove anything from the queue.
        let tree = self.db.open_tree(format!("tx_queue_chain_{}", chain_id))?;
        let prefix = tree.get("key_prefix")?.unwrap_or_else(|| b"tx".into());
        let mut queue = tree.scan_prefix(prefix);
        let (_, value) = match queue.next() {
            Some(Ok(v)) => v,
            _ => return Ok(None),
        };
        let tx = serde_json::from_slice(&value)?;
        Ok(Some(tx))
    }

    #[tracing::instrument(
        skip_all,
        fields(chain_id = %chain_id, tx_key = %hex::encode(key))
    )]
    fn has_tx(
        &self,
        chain_id: types::U256,
        key: &[u8],
    ) -> anyhow::Result<bool> {
        let tree = self.db.open_tree(format!("tx_queue_chain_{}", chain_id))?;
        tree.contains_key(key).map_err(Into::into)
    }

    #[tracing::instrument(
        skip_all,
        fields(chain_id = %chain_id, tx_key = %hex::encode(key))
    )]
    fn remove_tx(
        &self,
        chain_id: types::U256,
        key: &[u8],
    ) -> anyhow::Result<()> {
        let tree = self.db.open_tree(format!("tx_queue_chain_{}", chain_id))?;
        match tree.get(key)? {
            Some(k) => {
                let exists = tree.remove(k)?;
                debug_assert!(exists.is_some());
                tree.remove(key)?;
                tracing::trace!("removed tx from the queue..");
                self.db.flush()?;
                Ok(())
            }
            None => {
                // not found!
                anyhow::bail!(
                    "tx with key 0x{} not found in txqueue",
                    hex::encode(key)
                );
            }
        }
    }
}

impl ProposalStore for SledStore {
    #[tracing::instrument(
        skip_all,
        fields(data_hash = %hex::encode(&proposal.data_hash))
    )]
    fn insert_proposal(&self, proposal: ProposalEntity) -> anyhow::Result<()> {
        let tree = self.db.open_tree("proposal_store")?;
        tree.insert(
            &proposal.data_hash,
            serde_json::to_vec(&proposal)?.as_slice(),
        )?;
        tracing::trace!(
            "Saved Proposal @{} with resource_id = 0x{}",
            proposal.origin_chain_id,
            hex::encode(proposal.resource_id)
        );
        Ok(())
    }

    #[tracing::instrument(
        skip_all,
        fields(data_hash = %hex::encode(data_hash))
    )]
    fn remove_proposal(
        &self,
        data_hash: &[u8],
    ) -> anyhow::Result<Option<ProposalEntity>> {
        let tree = self.db.open_tree("proposal_store")?;
        match tree.get(&data_hash)? {
            Some(bytes) => {
                let proposal: ProposalEntity = serde_json::from_slice(&bytes)?;
                tracing::trace!(
                    "Removed Proposal @{} with resource_id = 0x{}",
                    proposal.origin_chain_id,
                    hex::encode(proposal.resource_id)
                );
                Ok(Some(proposal))
            }
            None => {
                tracing::warn!(
                    "Proposal not seen yet; not found in the proposal storage."
                );
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use webb::evm::ethers::prelude::transaction::eip2718::TypedTransaction;
    use webb::evm::ethers::types::transaction::request::TransactionRequest;

    #[test]
    fn get_leaves_should_work() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SledStore::open(tmp.path()).unwrap();
        let chain_id = types::U256::one();
        let contract =
            types::H160::from_slice("11111111111111111111".as_bytes());
        let block_number = types::U64::from(20);
        let default_block_number = types::U64::from(1);

        store
            .set_last_block_number((chain_id, contract), block_number)
            .unwrap();

        let block = store
            .get_last_block_number((contract, chain_id), default_block_number);

        match block {
            Ok(b) => {
                println!("retrieved block {:?}", block);
                assert_eq!(b, types::U64::from(20));
            }
            Err(e) => {
                println!("Error encountered {:?}", e);
            }
        }
    }

    #[test]
    fn tx_queue_should_work() {
        let tmp = tempfile::tempdir().unwrap();
        let store = SledStore::open(tmp.path()).unwrap();
        let chain_id = types::U256::one();
        // it is now empty
        assert_eq!(store.dequeue_tx(chain_id).unwrap(), None);

        let tx1: TypedTransaction = TransactionRequest::pay(
            types::Address::random(),
            types::U256::one(),
        )
        .from(types::Address::random())
        .into();
        store.enqueue_tx(chain_id, tx1.clone()).unwrap();
        let tx2: TypedTransaction = TransactionRequest::pay(
            types::Address::random(),
            types::U256::one(),
        )
        .from(types::Address::random())
        .into();
        store.enqueue_tx(chain_id, tx2.clone()).unwrap();

        // now let's dequeue transactions.
        assert_eq!(store.dequeue_tx(chain_id).unwrap(), Some(tx1));
        assert_eq!(store.dequeue_tx(chain_id).unwrap(), Some(tx2));

        let tx3: TypedTransaction = TransactionRequest::pay(
            types::Address::random(),
            types::U256::one(),
        )
        .from(types::Address::random())
        .into();
        store.enqueue_tx(chain_id, tx3.clone()).unwrap();
        assert_eq!(store.peek_tx(chain_id).unwrap(), Some(tx3.clone()));

        let tx3hash = tx3.sighash(chain_id.as_u64());
        assert!(store.has_tx(chain_id, tx3hash.as_bytes()).unwrap());
        store.remove_tx(chain_id, tx3hash.as_bytes()).unwrap();
        assert!(!store.has_tx(chain_id, tx3hash.as_bytes()).unwrap());
        assert_eq!(store.dequeue_tx(chain_id).unwrap(), None);
    }
}
