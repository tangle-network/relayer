#![allow(clippy::large_enum_variant)]

use std::convert::Infallible;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;

use async_stream::stream;
use futures::prelude::*;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use warp::ws::Message;
use webb::evm::contract::anchor::AnchorContract;
use webb::evm::ethereum_types::{Address, H256, U256};
use webb::evm::ethers::prelude::*;
use webb::evm::ethers::types::Bytes;

use crate::chains::evm;

use crate::context::RelayerContext;
use crate::leaf_cache::LeafCacheStore;

pub async fn accept_connection(
    ctx: &RelayerContext,
    stream: warp::ws::WebSocket,
) -> anyhow::Result<()> {
    let (mut tx, mut rx) = stream.split();
    while let Some(msg) = rx.try_next().await? {
        if let Ok(text) = msg.to_str() {
            handle_text(ctx, text, &mut tx).await?;
        }
    }
    Ok(())
}

pub async fn handle_text<TX>(
    ctx: &RelayerContext,
    v: &str,
    tx: &mut TX,
) -> anyhow::Result<()>
where
    TX: Sink<Message> + Unpin,
    TX::Error: Error + Send + Sync + 'static,
{
    match serde_json::from_str(v) {
        Ok(cmd) => {
            handle_cmd(ctx.clone(), cmd)
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

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpInformationResponse {
    ip: Option<SocketAddr>,
}

pub async fn handle_ip_info(
    ip: Option<SocketAddr>,
) -> Result<impl warp::Reply, Infallible> {
    Ok(warp::reply::json(&IpInformationResponse { ip }))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayerInformationResponse {
    #[serde(flatten)]
    config: crate::config::WebbRelayerConfig,
}

pub async fn handle_relayer_info(
    ctx: Arc<RelayerContext>,
) -> Result<impl warp::Reply, Infallible> {
    // clone the original config, to update it with accounts.
    let mut config = ctx.config.clone();
    /// Updates the account address in the provided network configuration.
    ///
    /// it takes the network name, as defined as a property in
    /// [`crate::config::WebbRelayerConfig`].
    /// and the [`evm::EvmChain`] to match on [`evm::ChainName`].
    macro_rules! update_account_for {
        ($c: expr, $f: tt, $network: ty) => {
            // first read the property (network) form the config, as mutable
            // but we also, at the same time require that we need the wallet
            // to be configured for that network, so we zip them together
            // in which either we get them both, or None.
            //
            // after this, we update the account property with the wallet
            // address.
            if let Some((c, w)) = $c
                .evm
                .$f
                .as_mut()
                .zip(ctx.evm_wallet::<$network>().await.ok())
            {
                c.account = Some(w.address());
            }
        };
    }

    update_account_for!(config, webb, evm::Webb);
    update_account_for!(config, ganache, evm::Ganache);
    update_account_for!(config, edgeware, evm::Edgeware);
    update_account_for!(config, beresheet, evm::Beresheet);
    update_account_for!(config, harmony, evm::Harmony);
    update_account_for!(config, rinkeby, evm::Rinkeby);

    Ok(warp::reply::json(&RelayerInformationResponse { config }))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LeavesCacheResponse {
    leaves: Vec<H256>,
}

pub async fn handle_leaves_cache(
    store: Arc<crate::leaf_cache::SledLeafCache>,
    contract: Address,
) -> Result<impl warp::Reply, Infallible> {
    let leaves = store.get_leaves(contract).unwrap();
    Ok(warp::reply::json(&LeavesCacheResponse { leaves }))
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
    Rinkeby(EvmRinkebyCommand),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubstrateEdgewareCommand {
    RelayWithdrew(),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvmEdgewareCommand {
    RelayWithdrew(EvmRelayerWithdrawProof),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubstrateBeresheetCommand {
    RelayWithdrew(),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvmBeresheetCommand {
    RelayWithdrew(EvmRelayerWithdrawProof),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubstrateWebbCommand {
    RelayWithdrew(),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvmWebbCommand {
    RelayWithdrew(EvmRelayerWithdrawProof),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvmRinkebyCommand {
    RelayWithdrew(EvmRelayerWithdrawProof),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SubstrateHedgewareCommand {
    RelayWithdrew(),
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
    Unimplemented(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NetworkStatus {
    Connecting,
    Connected,
    Failed { reason: String },
    Disconnected,
    UnsupportedContract,
    InvalidRelayerAddress,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WithdrawStatus {
    Sent,
    Submitted,
    Finlized {
        #[serde(rename = "txHash")]
        tx_hash: H256,
    },
    Valid,
    DroppedFromMemPool,
    Errored {
        reason: String,
    },
}

pub fn handle_cmd<'a>(
    ctx: RelayerContext,
    cmd: Command,
) -> BoxStream<'a, CommandResponse> {
    use CommandResponse::*;
    match cmd {
        Command::Substrate(_) => stream::once(async {
            Unimplemented("Substrate based networks are not implemented yet.")
        })
        .boxed(),
        Command::Evm(evm) => handle_evm(ctx, evm),
        Command::Ping() => stream::once(async { Pong() }).boxed(),
    }
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
            }
        },
        Harmony(c) => match c {
            EvmHarmonyCommand::RelayWithdrew(proof) => {
                handle_evm_withdrew::<evm::Harmony>(ctx, proof)
            }
        },
        Beresheet(c) => match c {
            EvmBeresheetCommand::RelayWithdrew(proof) => {
                handle_evm_withdrew::<evm::Beresheet>(ctx, proof)
            }
        },
        Ganache(c) => match c {
            EvmGanacheCommand::RelayWithdrew(proof) => {
                handle_evm_withdrew::<evm::Ganache>(ctx, proof)
            }
        },
        Webb(c) => match c {
            EvmWebbCommand::RelayWithdrew(proof) => {
                handle_evm_withdrew::<evm::Webb>(ctx, proof)
            }
        },
        Hedgeware(_) => todo!(),
        Rinkeby(c) => match c {
            EvmRinkebyCommand::RelayWithdrew(proof) => {
                handle_evm_withdrew::<evm::Rinkeby>(ctx, proof)
            }
        },
    };
    s.boxed()
}

fn handle_evm_withdrew<'a, C: evm::EvmChain>(
    ctx: RelayerContext,
    data: EvmRelayerWithdrawProof,
) -> BoxStream<'a, CommandResponse> {
    use CommandResponse::*;
    let s = stream! {
        let supported_contracts = C::contracts();
        if !supported_contracts.contains_key(&data.contract) {
            yield Network(NetworkStatus::UnsupportedContract);
            return;
        }
        let wallet = match ctx.evm_wallet::<C>().await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("Misconfigured Network: {}", e);
                yield Error(format!("Misconfigured Network: {:?}", C::name()));
                return;
            }
        };
        // validate the relayer address first before trying
        // send the transaction.
        let relayer_address = wallet.address();
        if (data.relayer != relayer_address) {
            yield Network(NetworkStatus::InvalidRelayerAddress);
            return;
        }

        tracing::debug!("Connecting to chain {:?} .. at {}", C::name(), C::endpoint());
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
                tracing::error!("Misconfigured Network: {}", e);
                yield Error(format!("Misconfigured Network: {:?}", C::name()));
                return;
            }
        };

        let client = SignerMiddleware::new(provider, wallet);
        let client = Arc::new(client);
        let contract = AnchorContract::new(data.contract, client);
        let denomination = match contract.denomination().call().await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("Misconfigured Contract Denomination: {}", e);
                yield Error(format!("Misconfigured Contract: {:?}", data.contract));
                return;
            }
        };
        let withdraw_fee_percentage = match ctx.fee_percentage::<C>(){
            Ok(v) => v,
            Err(e) => {
                tracing::error!("Misconfigured Fee in Config: {}", e);
                yield Error(format!("Misconfigured Fee: {:?}", C::name()));
                return;
            }
        };
        let expected_fee = calculate_fee(withdraw_fee_percentage, denomination);
        let (_, unacceptable_fee) = U256::overflowing_sub(data.fee, expected_fee);
        if unacceptable_fee {
            tracing::error!("Received a fee lower than configuration");
            let msg = format!(
                "User sent a fee that is too low {} but expected {}",
                data.fee,
                expected_fee,
            );
            yield Error(msg);
            return;
        }

        let call = contract.withdraw(
                data.proof.to_vec(),
                data.root.to_fixed_bytes(),
                data.nullifier_hash.to_fixed_bytes(),
                data.recipient,
                data.relayer,
                data.fee,
                data.refund
            );
        // Make a dry call, to make sure the transaction will go through successfully
        // to avoid wasting fees on invalid calls.
        match call.call().await {
            Ok(_) => {
                yield Withdraw(WithdrawStatus::Valid);
                tracing::debug!("Proof is valid");
            },
            Err(e) => {
                let reason = e.to_string();
                tracing::error!("Error Client sent an invalid proof: {}", reason);
                yield Withdraw(WithdrawStatus::Errored { reason });
                return;
            }
        };
        tracing::trace!("About to send Tx to {:?} Chain", C::name());
        let tx = match call.send().await {
            Ok(pending) => {
                yield Withdraw(WithdrawStatus::Sent);
                tracing::debug!("Tx is created! {}", *pending);
                let result = pending.await;
                tracing::debug!("Tx Submitted!");
                yield Withdraw(WithdrawStatus::Submitted);
                result
            },
            Err(e) => {
                let reason = e.to_string();
                tracing::error!("Error while sending Tx: {}", reason);
                yield Withdraw(WithdrawStatus::Errored { reason });
                return;
            }
        };
        match tx {
            Ok(Some(receipt)) => {
                tracing::debug!("Finlized Tx #{}", receipt.transaction_hash);
                yield Withdraw(WithdrawStatus::Finlized { tx_hash: receipt.transaction_hash });
            },
            Ok(None) => {
                tracing::warn!("Transaction Dropped from Mempool!!");
                yield Withdraw(WithdrawStatus::DroppedFromMemPool);
            }
            Err(e) => {
                let reason = e.to_string();
                tracing::error!("Transaction Errored: {}", reason);
                yield Withdraw(WithdrawStatus::Errored { reason });
            }
        };
    };
    s.boxed()
}

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
