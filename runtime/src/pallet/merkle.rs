use codec::{Decode, Encode};
use frame_support::Parameter;
use subxt::balances::*;
use subxt::sp_runtime::traits::AtLeast32Bit;
use subxt::system::*;

use super::*;

#[subxt::module]
pub trait Merkle: System + Balances {
    /// The overarching tree ID type
    type TreeId: 'static
        + Encode
        + Decode
        + Parameter
        + AtLeast32Bit
        + Default
        + Copy
        + Send
        + Sync;
}

// Storage ..

#[derive(Clone, Debug, Eq, Encode, PartialEq, subxt::Store)]
pub struct LeavesStore<T: Merkle> {
    #[store(returns = ScalarData)]
    tree_id: T::TreeId,
    index: u32,
}

impl<T: Merkle> LeavesStore<T> {
    pub fn try_get(tree_id: T::TreeId, index: u32) -> Self {
        Self { tree_id, index }
    }
}

#[derive(Clone, Debug, Eq, Encode, PartialEq, subxt::Store)]
pub struct CachedRootsStore<T: Merkle> {
    #[store(returns = Vec<ScalarData>)]
    block_number: T::BlockNumber,
    tree_id: T::TreeId,
}

impl<T: Merkle> CachedRootsStore<T> {
    pub fn new(block_number: T::BlockNumber, tree_id: T::TreeId) -> Self {
        Self {
            block_number,
            tree_id,
        }
    }
}
