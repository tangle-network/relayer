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
use std::collections::HashMap;
use std::convert::Infallible;
use std::error::Error;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use ethereum_types::{Address, H256, U256, U64};
use futures::prelude::*;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use warp::ws::Message;
use webb::evm::contract::protocol_solidity::fixed_deposit_anchor::Proof;
use webb::evm::contract::{
    protocol_solidity::{
        fixed_deposit_anchor::ExtData, FixedDepositAnchorContract,
    },
    tornado::TornadoContract,
};
use webb::evm::ethers::{
    contract::ContractError,
    core::k256::SecretKey,
    middleware::SignerMiddleware,
    providers::Middleware,
    signers::{LocalWallet, Signer},
    types::Bytes,
};
use webb::substrate::protocol_substrate_runtime::api::runtime_types::webb_standalone_runtime::Element;

use crate::context::RelayerContext;
use crate::store::LeafCacheStore;
use webb::substrate::protocol_substrate_runtime::api::RuntimeApi;
use webb::substrate::subxt::sp_core::Pair;
use webb::substrate::subxt::DefaultConfig;
use webb::substrate::subxt::{self, PairSigner, TransactionStatus};
/// Type alias for mpsc::Sender<CommandResponse>
type CommandStream = mpsc::Sender<CommandResponse>;

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
pub async fn handle_leaves_cache(
    store: Arc<crate::store::sled::SledStore>,
    chain_id: U256,
    contract: Address,
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
    Ok(warp::reply::json(&LeavesCacheResponse {
        leaves,
        last_queried_block,
    }))
}
/// Enumerates the supported commands for chain specific relayers
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Command {
    Substrate(SubstrateCommand),
    Evm(EvmCommand),
    Ping(),
}
/// Enumerates the supported commands for the substrate relayer
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubstrateCommand {
    MixerRelayTx(MixerRelayTransaction),
}
/// Contains data that is relayed to the Mixers
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MixerRelayTransaction {
    /// one of the supported chains of this relayer
    pub chain: String,
    /// The tree id of the mixer's underlying tree
    pub id: u32,
    /// The zero-knowledge proof bytes
    pub proof: Vec<u8>,
    /// The target merkle root for the proof
    pub root: [u8; 32],
    /// The nullifier_hash for the proof
    pub nullifier_hash: [u8; 32],
    /// The recipient of the transaction
    pub recipient: subxt::sp_core::crypto::AccountId32,
    /// The relayer of the transaction
    pub relayer: subxt::sp_core::crypto::AccountId32,
    /// The relayer's fee for the transaction
    pub fee: u128,
    /// The refund for the transaction in native tokens
    pub refund: u128,
}
/// Enumerates the supported EVM commands for relaying transactions
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvmCommand {
    TornadoRelayTx(TornadoRelayTransaction),
    AnchorRelayTx(AnchorRelayTransaction),
}
/// Contains the data for tornado relay transactions
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TornadoRelayTransaction {
    /// one of the supported chains of this relayer
    pub chain: String,
    /// The target contract.
    pub contract: Address,
    /// Proof bytes
    pub proof: Bytes,
    /// Args...
    pub root: H256,
    pub nullifier_hash: H256,
    pub recipient: Address, // H160 ([u8; 20])
    pub relayer: Address,   // H160 (should be this realyer account)
    pub fee: U256,
    pub refund: U256,
}
/// Contains transaction data that is relayed to Anchors
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnchorRelayTransaction {
    /// one of the supported chains of this relayer
    pub chain: String,
    /// The target contract.
    pub contract: Address,
    /// Proof bytes
    pub proof: Bytes,
    /// Args...
    pub roots: Bytes,
    pub refresh_commitment: H256,
    pub nullifier_hash: H256,
    pub ext_data_hash: H256,
    pub recipient: Address, // H160 ([u8; 20])
    pub relayer: Address,   // H160 (should be this realyer account)
    pub fee: U256,
    pub refund: U256,
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
    Misconfigured,
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
    match cmd {
        Command::Substrate(sub) => handle_substrate(ctx, sub, stream).await,
        Command::Evm(evm) => handle_evm(ctx, evm, stream).await,
        Command::Ping() => {
            let _ = stream.send(Pong()).await;
        }
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
        EvmCommand::TornadoRelayTx(cmd) => {
            handle_tornado_relay_tx(ctx, cmd, stream).await
        }
        EvmCommand::AnchorRelayTx(cmd) => {
            handle_anchor_relay_tx(ctx, cmd, stream).await
        }
    }
}
/// Handler for tornado commands
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `cmd` - The command to execute
/// * `stream` - The stream to write the response to
async fn handle_tornado_relay_tx<'a>(
    ctx: RelayerContext,
    cmd: TornadoRelayTransaction,
    stream: CommandStream,
) {
    use CommandResponse::*;
    let requested_chain = cmd.chain.to_lowercase();
    let chain = match ctx.config.evm.get(&requested_chain) {
        Some(v) => v,
        None => {
            tracing::event!(
                target: crate::probe::TARGET,
                tracing::Level::DEBUG,
                kind = %crate::probe::Kind::RelayTx,
                network = ?NetworkStatus::UnsupportedChain,
            );
            let _ = stream.send(Network(NetworkStatus::UnsupportedChain)).await;
            return;
        }
    };
    let supported_contracts: HashMap<_, _> = chain
        .contracts
        .iter()
        .cloned()
        .filter_map(|c| match c {
            crate::config::Contract::Tornado(c) => Some(c),
            _ => None,
        })
        .map(|c| (c.common.address, c))
        .collect();
    // get the contract configuration
    let contract_config = match supported_contracts.get(&cmd.contract) {
        Some(config) => config,
        None => {
            tracing::event!(
                target: crate::probe::TARGET,
                tracing::Level::DEBUG,
                kind = %crate::probe::Kind::RelayTx,
                network = ?NetworkStatus::UnsupportedContract,
            );
            let _ = stream
                .send(Network(NetworkStatus::UnsupportedContract))
                .await;
            return;
        }
    };

    let wallet = match ctx.evm_wallet(&cmd.chain).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("Misconfigured Network: {}", e);
            tracing::event!(
                target: crate::probe::TARGET,
                tracing::Level::DEBUG,
                kind = %crate::probe::Kind::RelayTx,
                network = ?NetworkStatus::Misconfigured,
            );
            let _ = stream
                .send(Error(format!("Misconfigured Network: {:?}", cmd.chain)))
                .await;
            return;
        }
    };
    // validate the relayer address first before trying
    // send the transaction.
    let reward_address = match chain.beneficiary {
        Some(account) => account,
        None => wallet.address(),
    };

    if cmd.relayer != reward_address {
        tracing::event!(
            target: crate::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %crate::probe::Kind::RelayTx,
            network = ?NetworkStatus::InvalidRelayerAddress,
        );
        let _ = stream
            .send(Network(NetworkStatus::InvalidRelayerAddress))
            .await;
        return;
    }

    tracing::debug!(
        "Connecting to chain {:?} .. at {}",
        cmd.chain,
        chain.http_endpoint
    );
    tracing::event!(
        target: crate::probe::TARGET,
        tracing::Level::DEBUG,
        kind = %crate::probe::Kind::RelayTx,
        network = ?NetworkStatus::Connecting,
    );
    let _ = stream.send(Network(NetworkStatus::Connecting)).await;
    let provider = match ctx.evm_provider(&cmd.chain).await {
        Ok(value) => {
            tracing::event!(
                target: crate::probe::TARGET,
                tracing::Level::DEBUG,
                kind = %crate::probe::Kind::RelayTx,
                network = ?NetworkStatus::Connected,
            );
            let _ = stream.send(Network(NetworkStatus::Connected)).await;
            value
        }
        Err(e) => {
            let reason = e.to_string();
            tracing::event!(
                target: crate::probe::TARGET,
                tracing::Level::DEBUG,
                kind = %crate::probe::Kind::RelayTx,
                network = "Failed",
                %reason,
            );
            let _ =
                stream.send(Network(NetworkStatus::Failed { reason })).await;
            tracing::event!(
                target: crate::probe::TARGET,
                tracing::Level::DEBUG,
                kind = %crate::probe::Kind::RelayTx,
                network = ?NetworkStatus::Disconnected,
            );
            let _ = stream.send(Network(NetworkStatus::Disconnected)).await;
            return;
        }
    };

    let client = SignerMiddleware::new(provider, wallet);
    let client = Arc::new(client);
    let contract = TornadoContract::new(cmd.contract, client);
    let denomination = match contract.denomination().call().await {
        Ok(v) => v,
        Err(e) => {
            tracing::event!(
                target: crate::probe::TARGET,
                tracing::Level::DEBUG,
                kind = %crate::probe::Kind::RelayTx,
                network = "Failed",
                reason = "Failed to get denomination from contract",
            );
            tracing::error!("Misconfigured Contract Denomination: {}", e);
            let _ = stream
                .send(Error(format!(
                    "Misconfigured Contract Denomination: {:?}",
                    e
                )))
                .await;
            return;
        }
    };
    // check the fee
    let expected_fee = calculate_fee(
        contract_config.withdraw_config.withdraw_fee_percentage,
        denomination,
    );
    let (_, unacceptable_fee) = U256::overflowing_sub(cmd.fee, expected_fee);
    if unacceptable_fee {
        tracing::error!("Received a fee lower than configuration");
        let msg = format!(
            "User sent a fee that is too low {} but expected {}",
            cmd.fee, expected_fee,
        );
        let _ = stream.send(Error(msg)).await;
        return;
    }

    let call = contract.withdraw(
        cmd.proof,
        cmd.root.to_fixed_bytes(),
        cmd.nullifier_hash.to_fixed_bytes(),
        cmd.recipient,
        cmd.relayer,
        cmd.fee,
        cmd.refund,
    );
    // Make a dry call, to make sure the transaction will go through successfully
    // to avoid wasting fees on invalid calls.
    match call.call().await {
        Ok(_) => {
            let _ = stream.send(Withdraw(WithdrawStatus::Valid)).await;
            tracing::debug!("Proof is valid");
        }
        Err(e) => {
            tracing::error!("Error Client sent an invalid proof: {}", e);
            let err = into_withdraw_error(e);
            let _ = stream.send(Withdraw(err)).await;
            return;
        }
    };
    tracing::trace!("About to send Tx to {:?} Chain", cmd.chain);
    let tx = match call.send().await {
        Ok(pending) => {
            let _ = stream.send(Withdraw(WithdrawStatus::Sent)).await;
            let tx_hash = *pending;
            tracing::debug!("Tx is submitted and pending! {}", tx_hash);
            let result = pending.interval(Duration::from_millis(1000)).await;
            let _ = stream
                .send(Withdraw(WithdrawStatus::Submitted { tx_hash }))
                .await;
            result
        }
        Err(e) => {
            tracing::error!("Error while sending Tx: {}", e);
            let err = into_withdraw_error(e);
            let _ = stream.send(Withdraw(err)).await;
            return;
        }
    };
    match tx {
        Ok(Some(receipt)) => {
            tracing::debug!("Finalized Tx #{}", receipt.transaction_hash);
            let _ = stream
                .send(Withdraw(WithdrawStatus::Finalized {
                    tx_hash: receipt.transaction_hash,
                }))
                .await;
        }
        Ok(None) => {
            tracing::warn!("Transaction Dropped from Mempool!!");
            let _ = stream
                .send(Withdraw(WithdrawStatus::DroppedFromMemPool))
                .await;
        }
        Err(e) => {
            let reason = e.to_string();
            tracing::error!("Transaction Errored: {}", reason);
            let _ = stream
                .send(Withdraw(WithdrawStatus::Errored { reason, code: 4 }))
                .await;
        }
    };
}
/// Handler for Anchor commands
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `cmd` - The command to execute
/// * `stream` - The stream to write the response to
async fn handle_anchor_relay_tx<'a>(
    ctx: RelayerContext,
    cmd: AnchorRelayTransaction,
    stream: CommandStream,
) {
    use CommandResponse::*;
    let requested_chain = cmd.chain.to_lowercase();
    let chain = match ctx.config.evm.get(&requested_chain) {
        Some(v) => v,
        None => {
            tracing::warn!("Unsupported Chain: {}", requested_chain);
            let _ = stream.send(Network(NetworkStatus::UnsupportedChain)).await;
            return;
        }
    };
    let supported_contracts: HashMap<_, _> = chain
        .contracts
        .iter()
        .cloned()
        .filter_map(|c| match c {
            crate::config::Contract::Anchor(c) => Some(c),
            _ => None,
        })
        .map(|c| (c.common.address, c))
        .collect();
    // get the contract configuration
    let contract_config = match supported_contracts.get(&cmd.contract) {
        Some(config) => config,
        None => {
            tracing::warn!("Unsupported Contract: {:?}", cmd.contract);
            let _ = stream
                .send(Network(NetworkStatus::UnsupportedContract))
                .await;
            return;
        }
    };

    let wallet = match ctx.evm_wallet(&cmd.chain).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("Misconfigured Network: {}", e);
            let _ = stream
                .send(Error(format!("Misconfigured Network: {:?}", cmd.chain)))
                .await;
            return;
        }
    };
    // validate the relayer address first before trying
    // send the transaction.
    let reward_address = match chain.beneficiary {
        Some(account) => account,
        None => wallet.address(),
    };

    if cmd.relayer != reward_address {
        let _ = stream
            .send(Network(NetworkStatus::InvalidRelayerAddress))
            .await;
        return;
    }

    // validate that the roots are multiple of 32s
    let roots = cmd.roots.to_vec();
    if roots.len() % 32 != 0 {
        let _ = stream
            .send(Withdraw(WithdrawStatus::InvalidMerkleRoots))
            .await;
        return;
    }

    tracing::debug!(
        "Connecting to chain {:?} .. at {}",
        cmd.chain,
        chain.http_endpoint
    );
    let _ = stream.send(Network(NetworkStatus::Connecting)).await;
    let provider = match ctx.evm_provider(&cmd.chain).await {
        Ok(value) => {
            let _ = stream.send(Network(NetworkStatus::Connected)).await;
            value
        }
        Err(e) => {
            let reason = e.to_string();
            let _ =
                stream.send(Network(NetworkStatus::Failed { reason })).await;
            let _ = stream.send(Network(NetworkStatus::Disconnected)).await;
            return;
        }
    };

    let client = SignerMiddleware::new(provider, wallet);
    let client = Arc::new(client);
    let contract = FixedDepositAnchorContract::new(cmd.contract, client);
    let denomination = match contract.denomination().call().await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("Misconfigured Contract Denomination: {}", e);
            let _ = stream
                .send(Error(format!(
                    "Misconfigured Contract: {:?}",
                    cmd.contract
                )))
                .await;
            return;
        }
    };
    // check the fee
    let expected_fee = calculate_fee(
        contract_config.withdraw_config.withdraw_fee_percentage,
        denomination,
    );
    let (_, unacceptable_fee) = U256::overflowing_sub(cmd.fee, expected_fee);
    if unacceptable_fee {
        tracing::error!("Received a fee lower than configuration");
        let msg = format!(
            "User sent a fee that is too low {} but expected {}",
            cmd.fee, expected_fee,
        );
        let _ = stream.send(Error(msg)).await;
        return;
    }

    let ext_data = ExtData {
        refresh_commitment: cmd.refresh_commitment.to_fixed_bytes(),
        recipient: cmd.recipient,
        relayer: cmd.relayer,
        fee: cmd.fee,
        refund: cmd.refund,
    };
    let proof = Proof {
        roots: roots.into(),
        proof: cmd.proof,
        nullifier_hash: cmd.nullifier_hash.to_fixed_bytes(),
        ext_data_hash: cmd.ext_data_hash.to_fixed_bytes(),
    };
    tracing::trace!(?proof, ?ext_data, "Client Proof");
    let call = contract.withdraw(proof, ext_data);
    // Make a dry call, to make sure the transaction will go through successfully
    // to avoid wasting fees on invalid calls.
    match call.call().await {
        Ok(_) => {
            let _ = stream.send(Withdraw(WithdrawStatus::Valid)).await;
            tracing::debug!("Proof is valid");
        }
        Err(e) => {
            tracing::error!("Error Client sent an invalid proof: {}", e);
            let err = into_withdraw_error(e);
            let _ = stream.send(Withdraw(err)).await;
            return;
        }
    };
    tracing::trace!("About to send Tx to {:?} Chain", cmd.chain);
    let tx = match call.send().await {
        Ok(pending) => {
            let _ = stream.send(Withdraw(WithdrawStatus::Sent)).await;
            let tx_hash = *pending;
            tracing::debug!(%tx_hash, "Tx is submitted and pending!");
            let result = pending.interval(Duration::from_millis(1000)).await;
            let _ = stream
                .send(Withdraw(WithdrawStatus::Submitted { tx_hash }))
                .await;
            result
        }
        Err(e) => {
            tracing::error!("Error while sending Tx: {}", e);
            let err = into_withdraw_error(e);
            let _ = stream.send(Withdraw(err)).await;
            return;
        }
    };
    match tx {
        Ok(Some(receipt)) => {
            tracing::debug!("Finalized Tx #{}", receipt.transaction_hash);
            let _ = stream
                .send(Withdraw(WithdrawStatus::Finalized {
                    tx_hash: receipt.transaction_hash,
                }))
                .await;
        }
        Ok(None) => {
            tracing::warn!("Transaction Dropped from Mempool!!");
            let _ = stream
                .send(Withdraw(WithdrawStatus::DroppedFromMemPool))
                .await;
        }
        Err(e) => {
            let reason = e.to_string();
            tracing::error!("Transaction Errored: {}", reason);
            let _ = stream
                .send(Withdraw(WithdrawStatus::Errored { reason, code: 4 }))
                .await;
        }
    };
}

fn into_withdraw_error<M: Middleware>(e: ContractError<M>) -> WithdrawStatus {
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
        SubstrateCommand::MixerRelayTx(cmd) => {
            handle_substrate_mixer_relay_tx(ctx, cmd, stream).await;
        }
    }
}
/// Handler for Substrate Mixer commands
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `cmd` - The command to execute
/// * `stream` - The stream to write the response to
async fn handle_substrate_mixer_relay_tx<'a>(
    ctx: RelayerContext,
    cmd: MixerRelayTransaction,
    stream: CommandStream,
) {
    use CommandResponse::*;

    let root_element = Element(cmd.root);
    let nullifier_hash_element = Element(cmd.nullifier_hash);

    let requested_chain = cmd.chain.to_lowercase();
    let maybe_client = ctx
        .substrate_provider::<DefaultConfig>(&requested_chain)
        .await;
    let client = match maybe_client {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Error while getting Substrate client: {}", e);
            let _ = stream.send(Error(format!("{}", e))).await;
            return;
        }
    };
    let api = client.to_runtime_api::<RuntimeApi<DefaultConfig, subxt::DefaultExtra<DefaultConfig>>>();

    let pair = match ctx.substrate_wallet(&cmd.chain).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("Misconfigured Network: {}", e);
            let _ = stream
                .send(Error(format!("Misconfigured Network: {:?}", cmd.chain)))
                .await;
            return;
        }
    };

    let signer = PairSigner::new(pair);

    let withdraw_tx = api
        .tx()
        .mixer_bn254()
        .withdraw(
            cmd.id,
            cmd.proof,
            root_element,
            nullifier_hash_element,
            cmd.recipient,
            cmd.relayer,
            cmd.fee,
            cmd.refund,
        )
        .sign_and_submit_then_watch(&signer)
        .await;
    let mut event_stream = match withdraw_tx {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Error while sending Tx: {}", e);
            let _ = stream.send(Error(format!("{}", e))).await;
            return;
        }
    };

    // Listen to the withdraw transaction, and send information back to the client
    loop {
        let maybe_event = event_stream.next().await;
        let event = match maybe_event {
            Some(Ok(v)) => v,
            Some(Err(e)) => {
                tracing::error!("Error while watching Tx: {}", e);
                let _ = stream.send(Error(format!("{}", e))).await;
                return;
            }
            None => break,
        };
        match event {
            TransactionStatus::Broadcast(_) => {
                let _ = stream.send(Withdraw(WithdrawStatus::Sent)).await;
            }
            TransactionStatus::InBlock(info) => {
                tracing::debug!(
                    "Transaction {:?} made it into block {:?}",
                    info.extrinsic_hash(),
                    info.block_hash()
                );
                let _ = stream
                    .send(Withdraw(WithdrawStatus::Submitted {
                        tx_hash: H256::from_slice(
                            info.extrinsic_hash().as_bytes(),
                        ),
                    }))
                    .await;
            }
            TransactionStatus::Finalized(info) => {
                tracing::debug!(
                    "Transaction {:?} finalized in block {:?}",
                    info.extrinsic_hash(),
                    info.block_hash()
                );
                let _has_event = match info.wait_for_success().await {
                    Ok(_) => {
                        // TODO: check if the event is actually a withdraw event
                        true
                    }
                    Err(e) => {
                        tracing::error!("Error while watching Tx: {}", e);
                        let _ = stream.send(Error(format!("{}", e))).await;
                        false
                    }
                };
                let _ = stream
                    .send(Withdraw(WithdrawStatus::Finalized {
                        tx_hash: H256::from_slice(
                            info.extrinsic_hash().as_bytes(),
                        ),
                    }))
                    .await;
            }
            TransactionStatus::Dropped => {
                tracing::warn!("Transaction dropped from the pool");
                let _ = stream
                    .send(Withdraw(WithdrawStatus::DroppedFromMemPool))
                    .await;
            }
            TransactionStatus::Invalid => {
                let _ = stream
                    .send(Withdraw(WithdrawStatus::Errored {
                        reason: "Invalid".to_string(),
                        code: 4,
                    }))
                    .await;
            }
            _ => continue,
        }
    }
}
/// Calculates the fee for a given transaction
fn calculate_fee(fee_percent: f64, principle: U256) -> U256 {
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
