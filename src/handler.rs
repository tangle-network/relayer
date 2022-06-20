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
//
#![allow(clippy::large_enum_variant)]
#![warn(missing_docs)]
use std::convert::Infallible;
use std::error::Error;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use ethereum_types::{Address, H256, U256, U64};
use futures::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use warp::ws::Message;
use webb::evm::ethers::prelude::I256;
use webb::evm::ethers::{
    contract::ContractError,
    core::k256::SecretKey,
    providers::Middleware,
    signers::{LocalWallet, Signer},
    types::Bytes,
};
use webb::substrate::subxt::sp_runtime::AccountId32;

use crate::context::RelayerContext;
use crate::store::LeafCacheStore;
use crate::tx_relay::evm::anchor::handle_anchor_relay_tx;
use crate::tx_relay::evm::vanchor::handle_vanchor_relay_tx;
use crate::tx_relay::substrate::anchor::handle_substrate_anchor_relay_tx;
use crate::tx_relay::substrate::mixer::handle_substrate_mixer_relay_tx;
use crate::tx_relay::substrate::vanchor::handle_substrate_vanchor_relay_tx;
use crate::tx_relay::{
    AnchorRelayTransaction, MixerRelayTransaction, VAnchorRelayTransaction,
};
use webb::substrate::subxt::sp_core::Pair;

/// Type alias for mpsc::Sender<CommandResponse>
pub type CommandStream = mpsc::Sender<CommandResponse>;
/// The command type for EVM txes
pub type EvmCommand = CommandType<
    Address, // Contract address
    Bytes,   // Proof bytes
    Bytes,   // Roots format
    H256,    // Element type
    Address, // Account identifier
    U256,    // Balance type
    I256,    // Signed amount type
>;
/// The command type for Substrate pallet txes
pub type SubstrateCommand = CommandType<
    u32,           // Tree Id
    Vec<u8>,       // Raw proof bytes
    Vec<[u8; 32]>, // Roots format
    [u8; 32],      // Element type
    AccountId32,   // Account identifier
    u128,          // Balance type
    i128,          // Signed amount type
>;

/// Sets up a websocket connection.
///
/// Returns `Ok(())` on success
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `stream` - Websocket stream
///
/// # Examples
///
/// ```
/// let _ = handler::accept_connection(ctx.as_ref(), socket).await;
/// ```
pub async fn accept_connection(
    ctx: &RelayerContext,
    stream: warp::ws::WebSocket,
) -> anyhow::Result<()> {
    let (mut tx, mut rx) = stream.split();

    // Wait for client to send over text (such as relay transaction requests)
    while let Some(msg) = rx.try_next().await? {
        if let Ok(text) = msg.to_str() {
            handle_text(ctx, text, &mut tx).await?;
        }
    }
    Ok(())
}
/// Sets up a websocket channels for message sending.
///
/// Returns `Ok(())` on success
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `v` - The text (usually in a JSON form) message to be handled.
/// * `tx` - A mutable Trait implementation of the `warp::ws::Sender` trait
///
/// # Examples
///
/// ```
/// let _ = handle_text(ctx, text, &mut tx).await?;;
/// ```
pub async fn handle_text<TX>(
    ctx: &RelayerContext,
    v: &str,
    tx: &mut TX,
) -> anyhow::Result<()>
where
    TX: Sink<Message> + Unpin,
    TX::Error: Error + Send + Sync + 'static,
{
    // for every connection, we create a new channel, where we will use to send messages
    // over it.
    let (my_tx, my_rx) = mpsc::channel(50);
    let res_stream = ReceiverStream::new(my_rx);
    match serde_json::from_str(v) {
        Ok(cmd) => {
            handle_cmd(ctx.clone(), cmd, my_tx).await;
            res_stream
                .fuse()
                .map(|v| serde_json::to_string(&v).expect("bad value"))
                .inspect(|v| tracing::trace!("Sending: {}", v))
                .map(Message::text)
                .map(Result::Ok)
                .forward(tx)
                .await?;
        }
        Err(e) => {
            tracing::warn!("Got invalid payload: {:?}", e);
            let error = CommandResponse::Error(e.to_string());
            let value = serde_json::to_string(&error)?;
            tx.send(Message::text(value)).await?
        }
    };
    Ok(())
}

/// Representation for IP address response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpInformationResponse {
    ip: String,
}
/// Handles the `ip` address response
///
/// Returns a Result with the `IpInformationResponse` on success
///
/// # Arguments
///
/// * `ip` - Option containing the IP address
///
/// # Examples
///
/// ```
/// let _ = handler::handle_ip_info
/// ```
pub async fn handle_ip_info(
    ip: Option<IpAddr>,
) -> Result<impl warp::Reply, Infallible> {
    Ok(warp::reply::json(&IpInformationResponse {
        ip: ip.unwrap().to_string(),
    }))
}
/// Handles the socket address response
///
/// Returns a Result with the `IpInformationResponse` on success
///
/// # Arguments
///
/// * `ip` - Option containing the socket address
///
/// # Examples
///
/// ```
/// let _ = handler::handle_ip_info
/// ```
pub async fn handle_socket_info(
    ip: Option<SocketAddr>,
) -> Result<impl warp::Reply, Infallible> {
    Ok(warp::reply::json(&IpInformationResponse {
        ip: ip.unwrap().ip().to_string(),
    }))
}
/// Handles relayer configuration requests
///
/// Returns a Result with the `RelayerConfigurationResponse` on success
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
pub async fn handle_relayer_info(
    ctx: Arc<RelayerContext>,
) -> Result<impl warp::Reply, Infallible> {
    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct RelayerInformationResponse {
        #[serde(flatten)]
        config: crate::config::WebbRelayerConfig,
    }
    // clone the original config, to update it with accounts.
    let mut config = ctx.config.clone();

    let _ = config
        .evm
        .values_mut()
        .filter(|v| v.beneficiary.is_none())
        .try_for_each(|v| {
            let key = SecretKey::from_bytes(v.private_key.as_bytes())?;
            let wallet = LocalWallet::from(key);
            v.beneficiary = Some(wallet.address());
            Result::<_, anyhow::Error>::Ok(())
        });
    let _ = config
        .substrate
        .values_mut()
        .filter(|v| v.beneficiary.is_none())
        .try_for_each(|v| {
            v.beneficiary = Some(v.suri.public());
            Result::<_, anyhow::Error>::Ok(())
        });
    Ok(warp::reply::json(&RelayerInformationResponse { config }))
}
/// Handles leaf data requests
///
/// Returns a Result with the `LeafDataResponse` on success
///
/// # Arguments
///
/// * `store` - [Sled](https://sled.rs)-based database store
/// * `chain_id` - An U256 representing the chain id of the chain to query
/// * `contract` - An address of the contract to query
/// * `is_data_query_enabled` - return response only if data query is enabled for relayer
pub async fn handle_leaves_cache(
    store: Arc<crate::store::sled::SledStore>,
    chain_id: U256,
    contract: Address,
    is_data_query_enabled: bool,
) -> Result<impl warp::Reply, Infallible> {
    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct LeavesCacheResponse {
        leaves: Vec<H256>,
        last_queried_block: U64,
    }
    let leaves = store.get_leaves((chain_id, contract)).unwrap();
    let last_queried_block = store
        .get_last_deposit_block_number((chain_id, contract))
        .unwrap();
    // check if data query is enabled for relayer
    if !is_data_query_enabled {
        tracing::warn!("Data query is not enabled for relayer.");
        return Ok(warp::reply::with_status(
            warp::reply::json(&String::from(
                "Data query is not enabled for relayer.",
            )),
            warp::http::StatusCode::FORBIDDEN,
        ));
    }
    Ok(warp::reply::with_status(
        warp::reply::json(&LeavesCacheResponse {
            leaves,
            last_queried_block,
        }),
        warp::http::StatusCode::OK,
    ))
}
/// Enumerates the supported commands for chain specific relayers
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Command {
    Substrate(SubstrateCommand),
    Evm(EvmCommand),
    Ping(),
}

/// Enumerates the supported protocols for relaying transactions
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CommandType<Id, P, R, E, I, B, A> {
    Mixer(MixerRelayTransaction<Id, P, E, I, B>),
    Anchor(AnchorRelayTransaction<Id, P, R, E, I, B>),
    VAnchor(VAnchorRelayTransaction<Id, P, R, E, I, B, A>),
}

/// Enumerates the command responses
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CommandResponse {
    Pong(),
    Network(NetworkStatus),
    Withdraw(WithdrawStatus),
    Error(String),
    #[allow(unused)]
    Unimplemented(&'static str),
}
/// Enumerates the network status response of the relayer
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NetworkStatus {
    Connecting,
    Connected,
    Failed { reason: String },
    Disconnected,
    UnsupportedContract,
    UnsupportedChain,
    InvalidRelayerAddress,
}
/// Enumerates the withdraw status response of the relayer
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WithdrawStatus {
    Sent,
    Submitted {
        #[serde(rename = "txHash")]
        tx_hash: H256,
    },
    Finalized {
        #[serde(rename = "txHash")]
        tx_hash: H256,
    },
    Valid,
    InvalidMerkleRoots,
    DroppedFromMemPool,
    Errored {
        code: i32,
        reason: String,
    },
}
/// Handles the command prompts for EVM and Substrate chains
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `cmd` - The command to execute
/// * `stream` - The stream to write the response to
pub async fn handle_cmd(
    ctx: RelayerContext,
    cmd: Command,
    stream: CommandStream,
) {
    use CommandResponse::*;
    if ctx.config.features.private_tx_relay {
        match cmd {
            Command::Substrate(sub) => handle_substrate(ctx, sub, stream).await,
            Command::Evm(evm) => handle_evm(ctx, evm, stream).await,
            Command::Ping() => {
                let _ = stream.send(Pong()).await;
            }
        }
    } else {
        tracing::error!("Private transaction relaying is not configured..!");
        let _ = stream
            .send(Error(
                "Private transaction relaying is not configured..!".to_string(),
            ))
            .await;
    }
}
/// Handler for EVM commands
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `cmd` - The command to execute
/// * `stream` - The stream to write the response to
pub async fn handle_evm(
    ctx: RelayerContext,
    cmd: EvmCommand,
    stream: CommandStream,
) {
    match cmd {
        CommandType::Anchor(_) => {
            handle_anchor_relay_tx(ctx, cmd, stream).await
        }
        CommandType::VAnchor(_) => {
            handle_vanchor_relay_tx(ctx, cmd, stream).await
        }
        _ => {}
    }
}

pub fn into_withdraw_error<M: Middleware>(
    e: ContractError<M>,
) -> WithdrawStatus {
    // a poor man error parser
    // WARNING: **don't try this at home**.
    let msg = format!("{}", e);
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
/// Handler for Substrate commands
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `cmd` - The command to execute
/// * `stream` - The stream to write the response to
pub async fn handle_substrate<'a>(
    ctx: RelayerContext,
    cmd: SubstrateCommand,
    stream: CommandStream,
) {
    match cmd {
        CommandType::Mixer(_) => {
            handle_substrate_mixer_relay_tx(ctx, cmd, stream).await;
        }
        CommandType::Anchor(_) => {
            handle_substrate_anchor_relay_tx(ctx, cmd, stream).await;
        }
        CommandType::VAnchor(_) => {
            handle_substrate_vanchor_relay_tx(ctx, cmd, stream).await;
        }
    }
}

/// Calculates the fee for a given transaction
pub fn calculate_fee(fee_percent: f64, principle: U256) -> U256 {
    let mill_fee = (fee_percent * 1_000_000.0) as u32;
    let mill_u256: U256 = principle * (mill_fee);
    let fee_u256: U256 = mill_u256 / (1_000_000);
    fee_u256
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn percent_fee() {
        let submitted_value =
            U256::from_dec_str("5000000000000000").ok().unwrap();
        let expected_fee = U256::from_dec_str("250000000000000").ok().unwrap();
        let withdraw_fee_percent_dec = 0.05f64;
        let formatted_fee =
            calculate_fee(withdraw_fee_percent_dec, submitted_value);

        assert_eq!(expected_fee, formatted_fee);
    }
}
