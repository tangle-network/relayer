#![allow(clippy::large_enum_variant)]

use std::convert::Infallible;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;

use async_stream::stream;
use chains::evm;
use futures::prelude::*;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use warp::ws::Message;
use webb::evm::contract::anchor::AnchorContract;
use webb::evm::ethereum_types::{Address, H256, U256};
use webb::evm::ethers::prelude::*;
use webb::evm::ethers::types::Bytes;

use crate::chains;

use crate::context::RelayerContext;

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
                .inspect(|v| log::trace!("Sending: {}", v))
                .map(Message::text)
                .map(Result::Ok)
                .forward(tx)
                .await?;
        },
        Err(e) => {
            log::warn!("Got invalid payload: {:?}", e);
            let error = CommandResponse::Error(e.to_string());
            let value = serde_json::to_string(&error)?;
            tx.send(Message::text(value)).await?
        },
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
        ($f: tt, $network: ty) => {
            // first read the property (network) form the config, as mutable
            // but we also, at the same time require that we need the wallet
            // to be configured for that network, so we zip them together
            // in which either we get them both, or None.
            //
            // after this, we update the account property with the wallet
            // address.
            if let Some((c, w)) = config
                .evm
                .$f
                .as_mut()
                .zip(ctx.evm_wallet::<$network>().await.ok())
            {
                c.account = Some(w.address());
            }
        };
    }

    update_account_for!(webb, evm::Webb);
    update_account_for!(ganache, evm::Ganache);
    update_account_for!(edgeware, evm::Edgeware);
    update_account_for!(beresheet, evm::Beresheet);
    update_account_for!(harmony, evm::Harmony);

    Ok(warp::reply::json(&RelayerInformationResponse { config }))
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
            },
        },
        Harmony(c) => match c {
            EvmHarmonyCommand::RelayWithdrew(proof) => {
                handle_evm_withdrew::<evm::Harmony>(ctx, proof)
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
            }
        };
    };
    s.boxed()
}

use parking_lot::RwLock;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// A Leaf Cache Store is a simple trait that would help in
/// getting the leaves and insert them with a simple API.
pub trait LeafCacheStore {
    type Output: IntoIterator<Item = H256>;

    fn get_leaves(&self, contract: Address) -> anyhow::Result<Self::Output>;

    fn insert_leaves(
        &self,
        contract: Address,
        leaves: &[H256],
    ) -> anyhow::Result<()>;

    fn set_last_block_number(&self, block_number: u64) -> anyhow::Result<()>;
    fn get_last_block_number(&self) -> anyhow::Result<u64>;
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryLeafCache {
    store: Arc<RwLock<HashMap<Address, Vec<H256>>>>,
    last_block_number: Arc<AtomicU64>,
}

impl InMemoryLeafCache {
    pub fn with_last_block_number(n: u64) -> Self {
        let this = Self::default();
        this.last_block_number.store(n, Ordering::Relaxed);
        this
    }
}

impl LeafCacheStore for InMemoryLeafCache {
    type Output = Vec<H256>;

    fn get_leaves(&self, contract: Address) -> anyhow::Result<Self::Output> {
        let guard = self.store.read();
        let val = guard.get(&contract).cloned().unwrap_or_default();
        Ok(val)
    }

    fn insert_leaves(
        &self,
        contract: Address,
        leaves: &[H256],
    ) -> anyhow::Result<()> {
        let mut guard = self.store.write();
        guard
            .entry(contract)
            .and_modify(|v| v.extend_from_slice(leaves))
            .or_insert_with(|| leaves.to_vec());
        Ok(())
    }

    fn get_last_block_number(&self) -> anyhow::Result<u64> {
        let val = self.last_block_number.load(Ordering::Relaxed);
        Ok(val)
    }

    fn set_last_block_number(&self, block_number: u64) -> anyhow::Result<()> {
        self.last_block_number
            .store(block_number, Ordering::Relaxed);
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct LeavesWatcher<C, S> {
    _chain: PhantomData<C>,
    store: S,
    contract: Address,
}

impl<C, S> LeavesWatcher<C, S>
where
    C: evm::EvmChain,
    S: LeafCacheStore,
{
    pub fn new(store: S, contract: Address) -> Self {
        Self {
            contract,
            store,
            _chain: PhantomData::default(),
        }
    }

    pub async fn watch(self) -> anyhow::Result<()> {
        let endpoint = C::endpoint();
        let fetch_interval = Duration::from_secs(6);
        let provider = Provider::try_from(endpoint)?.interval(fetch_interval);
        let client = Arc::new(provider);
        let contract = AnchorContract::new(self.contract, client);
        let block = self.store.get_last_block_number()?;
        let filter = contract.deposit_filter().from_block(block);
        let missing_events = filter.query().await?;
        dbg!(&missing_events);
        let missing_leaves = missing_events
            .into_iter()
            .map(|v| H256::from_slice(&v.commitment))
            .collect::<Vec<_>>();
        self.store.insert_leaves(self.contract, &missing_leaves)?;
        let mut events = filter.stream().await?;
        while let Some(event) = events.try_next().await? {
            dbg!(event);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use tokio::time::timeout;

    use super::*;

    #[tokio::test]
    async fn leaves_watcher() -> anyhow::Result<()> {
        let store = InMemoryLeafCache::with_last_block_number(1);
        let address =
            Address::from_str("0x8a4D675dcC71A7387a3C4f27d7D78834369b9542")?;
        let leaves_watcher =
            LeavesWatcher::<evm::Harmony, _>::new(store.clone(), address);
        let watcher = leaves_watcher.watch();
        if timeout(Duration::from_secs(60 * 60 * 60), watcher)
            .await
            .is_err()
        {
            println!("Timeout ..");
        }
        let leaves = store.get_leaves(address)?;
        dbg!(leaves);
        Ok(())
    }
}
