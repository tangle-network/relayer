use std::path::Path;

use webb::evm::ethers::types;

use super::{HistoryStore, LeafCacheStore};

#[derive(Clone)]
pub struct SledLeafCache {
    db: sled::Db,
}

impl std::fmt::Debug for SledLeafCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SledLeafCache").finish()
    }
}

impl SledLeafCache {
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

impl HistoryStore for SledLeafCache {
    #[tracing::instrument(skip(self))]
    fn set_last_block_number(
        &self,
        contract: types::Address,
        block_number: types::U64,
    ) -> anyhow::Result<types::U64> {
        let tree = self.db.open_tree("last_block_numbers")?;
        let mut bytes = [0u8; std::mem::size_of::<u64>()];
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

impl LeafCacheStore for SledLeafCache {
    type Output = Vec<types::H256>;

    #[tracing::instrument(skip(self))]
    fn get_leaves(
        &self,
        contract: types::Address,
    ) -> anyhow::Result<Self::Output> {
        let tree = self.db.open_tree(contract)?;
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
        let tree = self.db.open_tree(contract)?;
        for (k, v) in leaves {
            tree.insert(k.to_le_bytes(), v.as_bytes())?;
        }
        Ok(())
    }
}
