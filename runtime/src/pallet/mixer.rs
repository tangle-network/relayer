use std::marker::PhantomData;

use codec::{Decode, Encode};
use frame_support::Parameter;
use subxt::balances::*;
use subxt::system::*;

use crate::BalanceOf;

use super::*;

#[subxt::module]
pub trait Mixer: System + Balances + super::merkle::Merkle {
    #![event_type(DepositEvent<T>)]

    type ScalarData: Encode
        + Decode
        + Parameter
        + PartialEq
        + Eq
        + Default
        + Send
        + Sync
        + 'static;
    type Nullifier: Encode
        + Decode
        + Parameter
        + PartialEq
        + Eq
        + Default
        + Send
        + Sync
        + 'static;
    type Commitment: Encode
        + Decode
        + Parameter
        + PartialEq
        + Eq
        + Default
        + Send
        + Sync
        + 'static;
    type CurrencyId: Encode
        + Decode
        + Parameter
        + Eq
        + Default
        + Send
        + Sync
        + 'static;
}

/// Proof data for withdrawal
#[derive(Encode, Decode, PartialEq, Clone)]
pub struct WithdrawProof<T: Mixer> {
    /// The mixer id this withdraw proof corresponds to
    pub mixer_id: T::TreeId,
    /// The cached block for the cached root being proven against
    pub cached_block: T::BlockNumber,
    /// The cached root being proven against
    pub cached_root: ScalarData,
    /// The individual scalar commitments (to the randomness and nullifier)
    pub comms: Vec<Commitment>,
    /// The nullifier hash with itself
    pub nullifier_hash: ScalarData,
    /// The proof in bytes representation
    pub proof_bytes: Vec<u8>,
    /// The leaf index scalar commitments to decide on which side to hash
    pub leaf_index_commitments: Vec<Commitment>,
    /// The scalar commitments to merkle proof path elements
    pub proof_commitments: Vec<Commitment>,
    /// The recipient to withdraw amount of currency to
    pub recipient: Option<T::AccountId>,
    /// The recipient to withdraw amount of currency to
    pub relayer: Option<T::AccountId>,
}

// return types ..

#[derive(Encode, Decode, PartialEq, Debug)]
pub struct MixerInfo<T: Mixer> {
    pub minimum_deposit_length_for_reward: T::BlockNumber,
    pub fixed_deposit_size: BalanceOf<T>,
    pub currency_id: T::CurrencyId,
}

// Storage ..

#[derive(Clone, Debug, Eq, Encode, PartialEq, subxt::Store)]
pub struct MixerTreesStore<T: Mixer> {
    #[store(returns = MixerInfo<T>)]
    id: T::TreeId,
}

impl<T: Mixer> MixerTreesStore<T> {
    pub fn new(id: T::TreeId) -> Self { Self { id } }
}

#[derive(Clone, Debug, Eq, Encode, PartialEq, subxt::Store)]
pub struct MixerTreeIdsStore<T: Mixer> {
    #[store(returns = Vec<T::TreeId>)]
    __unused: PhantomData<T>,
}

impl<T: Mixer> Default for MixerTreeIdsStore<T> {
    fn default() -> Self {
        Self {
            __unused: PhantomData,
        }
    }
}

// Events ..

#[derive(Clone, Debug, Encode, Decode, Eq, PartialEq, subxt::Event)]
pub struct DepositEvent<T: Mixer> {
    tree_id: T::TreeId,
    account_id: T::AccountId,
    balance: BalanceOf<T>,
}

#[derive(Clone, Debug, Encode, Decode, Eq, PartialEq, subxt::Event)]
pub struct WithdrawEvent<T: Mixer> {
    group_id: T::TreeId,
    account_id: T::AccountId,
    nullifier: Nullifier,
}

// Calls ..

#[derive(Clone, Debug, Encode, Eq, PartialEq, subxt::Call)]
pub struct DepositCall<T: Mixer> {
    group_id: T::TreeId,
    data_points: Vec<ScalarData>,
}

#[derive(Clone, Encode, PartialEq, subxt::Call)]
pub struct WithdrawCall<T: Mixer> {
    withdraw_proof: WithdrawProof<T>,
}
