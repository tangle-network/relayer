use std::error::Error;
use std::sync::Arc;

use async_stream::stream;
use chains::evm;
use futures::prelude::*;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tungstenite::tokio::accept_async_with_config;
use tungstenite::tungstenite::protocol::WebSocketConfig;
use tungstenite::tungstenite::Message;
use webb::contracts::anchor::AnchorContract;
use webb::evm::ethereum_types::{Address, H256, U256};
use webb::evm::ethers::prelude::*;
use webb::evm::ethers::types::Bytes;
use webb::pallet::merkle::Merkle;
use webb::pallet::mixer::{self, *};
use webb::pallet::*;
use webb::substrate::subxt::sp_runtime::AccountId32;

use crate::chains;

use crate::context::RelayerContext;

pub async fn accept_connection(
    ctx: RelayerContext,
    stream: TcpStream,
) -> anyhow::Result<()> {
    let config = WebSocketConfig {
        max_send_queue: Some(5),
        max_message_size: Some(2 << 20), // 5MB
        ..Default::default()
    };
    let ws_stream = accept_async_with_config(stream, Some(config)).await?;
    let (mut tx, mut rx) = ws_stream.split();
    while let Some(msg) = rx.try_next().await? {
        match msg {
            Message::Text(v) => handle_text(&ctx, v, &mut tx).await?,
            Message::Binary(_) => {
                // should we close the connection?
            },
            _ => continue,
        }
    }
    Ok(())
}

pub async fn handle_text<TX>(
    ctx: &RelayerContext,
    v: String,
    tx: &mut TX,
) -> anyhow::Result<()>
where
    TX: Sink<Message> + Unpin,
    TX::Error: Error + Send + Sync + 'static,
{
    match serde_json::from_str(&v) {
        Ok(cmd) => {
            handle_cmd(ctx.clone(), cmd)
                .map(|v| serde_json::to_string(&v).expect("bad value"))
                .map(Message::Text)
                .map(Result::Ok)
                .forward(tx)
                .await?;
        },
        Err(e) => {
            log::warn!("Got invalid payload: {:?}", e);
            let error = CommandResponse::Error(e.to_string());
            let value = serde_json::to_string(&error)?;
            tx.send(Message::Text(value)).await?
        },
    };
    Ok(())
}

/// Proof data for withdrawal
#[derive(Debug, PartialEq, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubstrateRelayerWithdrawProof {
    /// The mixer id this withdraw proof corresponds to
    pub mixer_id: u32,
    /// The cached block for the cached root being proven against
    pub cached_block: u32,
    /// The cached root being proven against
    pub cached_root: [u8; 32],
    /// The individual scalar commitments (to the randomness and nullifier)
    pub comms: Vec<[u8; 32]>,
    /// The nullifier hash with itself
    pub nullifier_hash: [u8; 32],
    /// The proof in bytes representation
    pub proof_bytes: Vec<u8>,
    /// The leaf index scalar commitments to decide on which side to hash
    pub leaf_index_commitments: Vec<[u8; 32]>,
    /// The scalar commitments to merkle proof path elements
    pub proof_commitments: Vec<[u8; 32]>,
    /// The recipient to withdraw amount of currency to
    pub recipient: Option<[u8; 32]>,
    /// The recipient to withdraw amount of currency to
    pub relayer: Option<[u8; 32]>,
}

impl<T> From<SubstrateRelayerWithdrawProof> for mixer::WithdrawProof<T>
where
    T: Merkle + Mixer,
    T::TreeId: From<u32>,
    T::BlockNumber: From<u32>,
    T::AccountId: From<AccountId32>,
{
    fn from(p: SubstrateRelayerWithdrawProof) -> Self {
        Self {
            mixer_id: p.mixer_id.into(),
            cached_block: p.cached_block.into(),
            cached_root: ScalarData(p.cached_root),
            comms: p.comms.into_iter().map(Commitment).collect(),
            nullifier_hash: ScalarData(p.nullifier_hash),
            proof_bytes: p.proof_bytes,
            leaf_index_commitments: p
                .leaf_index_commitments
                .into_iter()
                .map(Commitment)
                .collect(),
            proof_commitments: p
                .proof_commitments
                .into_iter()
                .map(Commitment)
                .collect(),
            recipient: p.recipient.map(AccountId32::new).map(Into::into),
            relayer: p.relayer.map(AccountId32::new).map(Into::into),
        }
    }
}

impl<T> From<mixer::WithdrawProof<T>> for SubstrateRelayerWithdrawProof
where
    T: Merkle + Mixer,
    T::TreeId: Into<u32>,
    T::BlockNumber: Into<u32>,
    T::AccountId: Into<AccountId32>,
{
    fn from(p: mixer::WithdrawProof<T>) -> Self {
        Self {
            mixer_id: p.mixer_id.into(),
            cached_block: p.cached_block.into(),
            cached_root: p.cached_root.0,
            comms: p.comms.into_iter().map(|v| v.0).collect(),
            nullifier_hash: p.nullifier_hash.0,
            proof_bytes: p.proof_bytes,
            leaf_index_commitments: p
                .leaf_index_commitments
                .into_iter()
                .map(|v| v.0)
                .collect(),
            proof_commitments: p
                .proof_commitments
                .into_iter()
                .map(|v| v.0)
                .collect(),
            recipient: p.recipient.map(|v| *v.into().as_ref()),
            relayer: p.relayer.map(|v| *v.into().as_ref()),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Command {
    Substrate(SubstrateCommand),
    Evm(EvmCommand),
    Ping(),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubstrateCommand {
    Edgeware(SubstrateEdgewareCommand),
    Beresheet(SubstrateBeresheetCommand),
    Hedgeware(SubstrateHedgewareCommand),
    Webb(SubstrateWebbCommand),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvmCommand {
    Edgeware(EvmEdgewareCommand),
    Harmony(EvmHarmonyCommand),
    Beresheet(EvmBeresheetCommand),
    Ganache(EvmGanacheCommand),
    Hedgeware(EvmHedgewareCommand),
    Webb(EvmWebbCommand),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubstrateEdgewareCommand {
    RelayWithdrew(SubstrateRelayerWithdrawProof),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvmEdgewareCommand {
    RelayWithdrew(EvmRelayerWithdrawProof),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubstrateBeresheetCommand {
    RelayWithdrew(SubstrateRelayerWithdrawProof),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvmBeresheetCommand {
    RelayWithdrew(EvmRelayerWithdrawProof),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubstrateWebbCommand {
    RelayWithdrew(SubstrateRelayerWithdrawProof),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvmWebbCommand {
    RelayWithdrew(EvmRelayerWithdrawProof),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubstrateHedgewareCommand {
    RelayWithdrew(SubstrateRelayerWithdrawProof),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvmHedgewareCommand {
    RelayWithdrew(EvmRelayerWithdrawProof),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvmGanacheCommand {
    RelayWithdrew(EvmRelayerWithdrawProof),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvmHarmonyCommand {
    RelayWithdrew(EvmRelayerWithdrawProof),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmRelayerWithdrawProof {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CommandResponse {
    Pong(),
    Network(NetworkStatus),
    Withdraw(WithdrawStatus),
    Error(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NetworkStatus {
    Connecting,
    Connected,
    Failed { reason: String },
    Disconnected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WithdrawStatus {
    Sent,
    Submitted,
    Finlized { tx_hash: H256 },
    Errored { reason: String },
}
pub fn handle_cmd<'a>(
    ctx: RelayerContext,
    cmd: Command,
) -> BoxStream<'a, CommandResponse> {
    use CommandResponse::*;
    match cmd {
        Command::Substrate(sub) => handle_substrate(ctx, sub),
        Command::Evm(evm) => handle_evm(ctx, evm),
        Command::Ping() => stream::once(async { Pong() }).boxed(),
    }
}

pub fn handle_substrate<'a>(
    ctx: RelayerContext,
    cmd: SubstrateCommand,
) -> BoxStream<'a, CommandResponse> {
    let s = stream! {
        yield CommandResponse::Pong();
    };
    s.boxed()
}

pub fn handle_evm<'a>(
    ctx: RelayerContext,
    cmd: EvmCommand,
) -> BoxStream<'a, CommandResponse> {
    use EvmCommand::*;
    let s = match cmd {
        Edgeware(c) => match c {
            EvmEdgewareCommand::RelayWithdrew(proof) => {
                handle_evm_withdrew::<evm::Edgeware>(ctx, proof)
            },
        },
        Harmony(c) => match c {
            EvmHarmonyCommand::RelayWithdrew(proof) => {
                handle_evm_withdrew::<evm::Harmoney>(ctx, proof)
            },
        },
        Beresheet(c) => match c {
            EvmBeresheetCommand::RelayWithdrew(proof) => {
                handle_evm_withdrew::<evm::Beresheet>(ctx, proof)
            },
        },
        Ganache(c) => match c {
            EvmGanacheCommand::RelayWithdrew(proof) => {
                handle_evm_withdrew::<evm::Ganache>(ctx, proof)
            },
        },
        Webb(c) => match c {
            EvmWebbCommand::RelayWithdrew(proof) => {
                handle_evm_withdrew::<evm::Webb>(ctx, proof)
            },
        },
        Hedgeware(_) => todo!(),
    };
    s.boxed()
}

fn handle_evm_withdrew<'a, C: evm::EvmChain>(
    ctx: RelayerContext,
    data: EvmRelayerWithdrawProof,
) -> BoxStream<'a, CommandResponse> {
    use CommandResponse::*;
    let s = stream! {
        log::debug!("Connecting to chain {:?} .. at {}", C::name(), C::endpoint());
        yield Network(NetworkStatus::Connecting);
        let provider = match ctx.evm_provider::<C>().await {
            Ok(value) => {
                yield Network(NetworkStatus::Connected);
                value
            },
            Err(e) => {
                let reason = e.to_string();
                yield Network(NetworkStatus::Failed { reason });
                yield Network(NetworkStatus::Disconnected);
                return;
            }
        };
        let wallet = match ctx.evm_wallet::<C>().await {
            Ok(v) => v,
            Err(e) => {
                log::error!("Misconfigured Network: {}", e);
                yield Error(format!("Misconfigured Network: {:?}", C::name()));
                return;
            }
        };
        let client = SignerMiddleware::new(provider, wallet);
        let client = Arc::new(client);
        let contract = AnchorContract::new(data.contract, client);
        let call = contract.withdraw(
                data.proof.to_vec(),
                data.root.to_fixed_bytes(),
                data.nullifier_hash.to_fixed_bytes(),
                data.recipient,
                data.relayer,
                data.fee,
                data.refund
            );
        log::trace!("About to send Tx to {:?} Chain", C::name());
        let tx = match call.send().await {
            Ok(pending) => {
                yield Withdraw(WithdrawStatus::Sent);
                log::debug!("Tx is created! {}", *pending);
                let result = pending.await;
                log::debug!("Tx Submitted!");
                yield Withdraw(WithdrawStatus::Submitted);
                result
            },
            Err(e) => {
                let reason = e.to_string();
                log::error!("Error while sending Tx: {}", reason);
                yield Withdraw(WithdrawStatus::Errored { reason });
                return;
            }
        };
        match tx {
            Ok(receipt) => {
                log::debug!("Finlized Tx #{}", receipt.transaction_hash);
                yield Withdraw(WithdrawStatus::Finlized { tx_hash: receipt.transaction_hash });
            },
            Err(e) => {
                let reason = e.to_string();
                log::error!("Transaction Errored: {}", reason);
                yield Withdraw(WithdrawStatus::Errored { reason });
                return;
            }
        };
    };
    s.boxed()
}
