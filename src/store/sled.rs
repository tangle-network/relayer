use std::path::Path;

use webb::evm::ethers::types;

use super::{HistoryStore, LeafCacheStore};

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
    fn set_last_block_number(
        &self,
        contract: types::Address,
        block_number: types::U64,
    ) -> anyhow::Result<types::U64> {
        let tree = self.db.open_tree("last_block_numbers")?;
        let mut bytes = [0u8; std::mem::size_of::<types::U64>()];
        block_number.to_little_endian(&mut bytes);
        let old = tree.insert(contract, &bytes)?;
        match old {
            Some(v) => Ok(types::U64::from_little_endian(&v)),
            None => Ok(block_number),
        }
    }

    #[tracing::instrument(skip(self))]
    fn get_last_block_number(
        &self,
        contract: types::Address,
        default_block_number: types::U64,
    ) -> anyhow::Result<types::U64> {
        let tree = self.db.open_tree("last_block_numbers")?;
        let val = tree.get(contract)?;
        match val {
            Some(v) => Ok(types::U64::from_little_endian(&v)),
            None => Ok(default_block_number),
        }
    }
}

impl LeafCacheStore for SledStore {
    type Output = Vec<types::H256>;

    #[tracing::instrument(skip(self))]
    fn get_leaves(
        &self,
        contract: types::Address,
    ) -> anyhow::Result<Self::Output> {
        let tree = self.db.open_tree(format!("leaves/{}", contract))?;
        let leaves = tree
            .iter()
            .values()
            .flatten()
            .map(|v| types::H256::from_slice(&v))
            .collect();
        Ok(leaves)
    }

    #[tracing::instrument(skip(self))]
    fn insert_leaves(
        &self,
        contract: types::Address,
        leaves: &[(u32, types::H256)],
    ) -> anyhow::Result<()> {
        let tree = self.db.open_tree(format!("leaves/{}", contract))?;
        for (k, v) in leaves {
            tree.insert(k.to_le_bytes(), v.as_bytes())?;
        }
        Ok(())
    }

    fn get_last_deposit_block_number(
        &self,
        contract: types::Address,
    ) -> anyhow::Result<types::U64> {
        let tree = self.db.open_tree("last_deposit")?;
        let val = tree.get(contract)?;
        match val {
            Some(v) => Ok(types::U64::from_little_endian(&v)),
            None => Ok(types::U64::from(0)),
        }
    }

    fn insert_last_deposit_block_number(
        &self,
        contract: types::Address,
        last_deposit: types::U64,
    ) -> anyhow::Result<types::U64> {
        let tree = self.db.open_tree("last_deposit")?;
        let mut bytes = [0u8; std::mem::size_of::<types::U64>()];
        last_deposit.to_little_endian(&mut bytes);
        let old = tree.insert(contract, &bytes)?;
        match old {
            Some(v) => Ok(types::U64::from_little_endian(&v)),
            None => Ok(last_deposit),
        }
    }
}
