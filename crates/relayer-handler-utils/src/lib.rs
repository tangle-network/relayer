// Copyright 2022 Webb Technologies Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![allow(clippy::large_enum_variant)]
#![allow(missing_docs)]

use serde::{Deserialize, Deserializer, Serialize};
use tokio::sync::mpsc;
use webb::evm::ethers::abi::Address;
use webb::evm::ethers::prelude::{ContractError, I256, U128};
use webb::evm::ethers::providers::Middleware;
use webb::evm::ethers::types::Bytes;
use webb::evm::ethers::types::{H256, U256};
use webb::substrate::subxt::utils::AccountId32;
use webb_relayer_tx_relay_utils::{
    MixerRelayTransaction, VAnchorRelayTransaction,
};

/// Representation for IP address response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpInformationResponse {
    pub ip: String,
}

/// A wrapper type around [`I256`] that implements a correct way for [`Serialize`] and [`Deserialize`].
///
/// This supports the signed integer hex values that are not originally supported by the [`I256`] type.
#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct WebbI256(pub I256);

impl<'de> Deserialize<'de> for WebbI256 {
    fn deserialize<D>(deserializer: D) -> Result<WebbI256, D::Error>
    where
        D: Deserializer<'de>,
    {
        let i128_str = String::deserialize(deserializer)?;
        let i128_val =
            I256::from_hex_str(&i128_str).map_err(serde::de::Error::custom)?;
        Ok(WebbI256(i128_val))
    }
}

/// Type of Command to use
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Command {
    /// Substrate specific subcommand.
    Substrate(SubstrateCommandType),
    /// EVM specific subcommand.
    Evm(EvmCommandType),
    /// Ping?
    Ping(),
}

/// Enumerates the supported evm commands for relaying transactions
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvmCommandType {
    /// Webb Variable Anchors.
    VAnchor(EvmVanchorCommand),
}

/// Enumerates the supported substrate commands for relaying transactions
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubstrateCommandType {
    /// Webb Mixer.
    Mixer(SubstrateMixerCommand),
    /// Webb Variable Anchors.
    VAnchor(SubstrateVAchorCommand),
}

/// Enumerates the command responses
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CommandResponse {
    /// Pong?
    Pong(),
    /// Network Status
    Network(NetworkStatus),
    /// Withdrawal Status
    Withdraw(WithdrawStatus),
    /// An error occurred
    Error(String),
}
/// Enumerates the network status response of the relayer
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NetworkStatus {
    /// Relayer is connecting to the network.
    Connecting,
    /// Relayer is connected to the network.
    Connected,
    /// Network failure with error message.
    Failed {
        /// Error message
        reason: String,
    },
    /// Relayer is disconnected from the network.
    Disconnected,
    /// This contract is not supported by the relayer.
    UnsupportedContract,
    /// This network (chain) is not supported by the relayer.
    UnsupportedChain,
    /// Invalid Relayer address in the proof
    InvalidRelayerAddress,
}
/// Enumerates the withdraw status response of the relayer
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WithdrawStatus {
    /// The transaction is sent to the network.
    Sent,
    /// The transaction is submitted to the network.
    Submitted {
        /// The transaction hash.
        #[serde(rename = "txHash")]
        tx_hash: H256,
    },
    /// The transaction is in the block.
    Finalized {
        /// The transaction hash.
        #[serde(rename = "txHash")]
        tx_hash: H256,
    },
    /// Valid transaction.
    Valid,
    /// Invalid Merkle roots.
    InvalidMerkleRoots,
    /// Transaction dropped from mempool, send it again.
    DroppedFromMemPool,
    /// Invalid transaction.
    Errored {
        /// Error Code.
        code: i32,
        /// Error Message.
        reason: String,
    },
}

/// Type alias for mpsc::Sender<CommandResponse>
pub type CommandStream = mpsc::Sender<CommandResponse>;
/// The command type for EVM vanchor transactions
pub type EvmVanchorCommand = VAnchorRelayTransaction<
    Address,  // Contract address
    Bytes,    // Proof bytes
    Bytes,    // Roots format
    H256,     // Element type
    Address,  // Account identifier
    U256,     // Balance type
    WebbI256, // Signed amount type
    Address,  // Token Address
>;

type Id = u32; //  Substrate tree Id
type P = Vec<u8>; // Substrate raw proof bytes
type R = Vec<[u8; 32]>; // Substrate roots format
type E = [u8; 32]; // Substrate element type
type I = AccountId32; // Substrate account identifier
type B = U128; // Substrate balance type
               // TODO: this should use I128 type (but doesnt exist)
type A = i128; // Substrate signed amount type
type T = u32; // Substrate assetId

/// The command type for Substrate mixer txes
pub type SubstrateMixerCommand = MixerRelayTransaction<Id, P, E, I, B>;
/// The command type for Substrate vanchor txes
pub type SubstrateVAchorCommand =
    VAnchorRelayTransaction<Id, P, R, E, I, B, A, T>;

/// A helper function to extract the error code and the reason from EVM errors.
pub fn into_withdraw_error<M: Middleware>(
    e: ContractError<M>,
) -> WithdrawStatus {
    // a poor man error parser
    // WARNING: **don't try this at home**.
    let msg = format!("{e}");
    // split the error into words, lazily.
    let mut words = msg.split_whitespace();
    let mut reason = "unknown".to_string();
    let mut code = -1;

    while let Some(current_word) = words.next() {
        if current_word == "(code:" {
            code = match words.next() {
                Some(val) => {
                    let mut v = val.to_string();
                    v.pop(); // remove ","
                    v.parse().unwrap_or(-1)
                }
                _ => -1, // unknown code
            };
        } else if current_word == "message:" {
            // next we need to collect all words in between "message:"
            // and "data:", that would be the error message.
            let msg: Vec<_> =
                words.clone().take_while(|v| *v != "data:").collect();
            reason = msg.join(" ");
            reason.pop(); // remove the "," at the end.
        }
    }

    WithdrawStatus::Errored { reason, code }
}
