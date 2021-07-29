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
        }
        Err(e) => {
            log::warn!("Got invalid payload: {:?}", e);
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
            Ok(Some(receipt)) => {
                log::debug!("Finlized Tx #{}", receipt.transaction_hash);
                yield Withdraw(WithdrawStatus::Finlized { tx_hash: receipt.transaction_hash });
            },
            Ok(None) => {
                log::warn!("Transaction Dropped from Mempool!!");
                yield Withdraw(WithdrawStatus::DroppedFromMemPool);
            }
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
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use webb::evm::ethers::providers::Ws;

/// A Leaf Cache Store is a simple trait that would help in
/// getting the leaves and insert them with a simple API.
pub trait LeafCacheStore {
    type Output: IntoIterator<Item = H256>;

    fn get_leaves(&self, contract: Address) -> anyhow::Result<Self::Output>;

    fn insert_leaves(
        &self,
        contract: Address,
        leaves: &[(u32, H256)],
    ) -> anyhow::Result<()>;
    /// Sets the new block number for the cache and returns the old one.
    fn set_last_block_number(&self, block_number: u64) -> anyhow::Result<u64>;
    fn get_last_block_number(&self) -> anyhow::Result<u64>;
}

type MemStore = HashMap<Address, Vec<(u32, H256)>>;

#[derive(Debug, Clone, Default)]
pub struct InMemoryLeafCache {
    store: Arc<RwLock<MemStore>>,
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
        let val = guard
            .get(&contract)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|v| v.1)
            .collect();
        Ok(val)
    }

    fn insert_leaves(
        &self,
        contract: Address,
        leaves: &[(u32, H256)],
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

    fn set_last_block_number(&self, block_number: u64) -> anyhow::Result<u64> {
        let old = self.last_block_number.swap(block_number, Ordering::Relaxed);
        Ok(old)
    }
}

#[derive(Debug, Clone)]
pub struct LeavesWatcher<S> {
    ws_endpoint: String,
    store: S,
    contract: Address,
}

impl<S> LeavesWatcher<S>
where
    S: LeafCacheStore,
{
    pub fn new(
        ws_endpoint: impl Into<String>,
        store: S,
        contract: Address,
    ) -> Self {
        Self {
            ws_endpoint: ws_endpoint.into(),
            contract,
            store,
        }
    }

    pub async fn watch(self) -> anyhow::Result<()> {
        log::debug!("Connecting to {}", self.ws_endpoint);
        let ws = Ws::connect(&self.ws_endpoint).await?;
        let fetch_interval = Duration::from_millis(200);
        let provider = Provider::new(ws).interval(fetch_interval);
        let client = Arc::new(provider);
        let contract = AnchorContract::new(self.contract, client.clone());
        let block = self.store.get_last_block_number()?;
        log::debug!("Starting from block {}", block + 1);
        let filter = contract.deposit_filter().from_block(block + 1);
        let missing_events = filter.query_with_meta().await?;
        log::debug!("Got #{} missing events", missing_events.len());
        for (e, log) in missing_events {
            self.store.insert_leaves(
                self.contract,
                &[(e.leaf_index, H256::from_slice(&e.commitment))],
            )?;
            let old = self
                .store
                .set_last_block_number(log.block_number.as_u64())?;
            log::debug!(
                "Going from #{} to #{}",
                old,
                log.block_number.as_u64()
            );
        }
        let mut events = filter.subscribe().await?;
        while let Some(e) = events.try_next().await? {
            self.store.insert_leaves(
                self.contract,
                &[(e.leaf_index, H256::from_slice(&e.commitment))],
            )?;
            // FIXME(@shekohex): currently we fetch events from the stream
            // but we never have a way to get the current block number from where this event occurred.
            // as for now, we will get the latest block number and just assume that it
            // is the same block number this event occurred.
            let last_block_number = client.get_block_number().await?;
            log::debug!("Last block number: {}", last_block_number);
            self.store
                .set_last_block_number(last_block_number.as_u64())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use std::time::Duration;

    use anyhow::Context;
    use ethers::abi::Tokenizable;
    use ethers::utils::{Ganache, GanacheInstance};
    use rand::SeedableRng;
    use webb::evm::ethers;
    use webb::evm::ethers::prelude::*;
    use webb::evm::note::{Deposit, Note};

    use super::*;

    fn create_contract_factory<P: Into<PathBuf>, M: Middleware>(
        path: P,
        client: Arc<M>,
    ) -> anyhow::Result<ContractFactory<M>> {
        let json_file = fs::read_to_string(path.into())
            .context("reading contract json file")?;
        let raw: serde_json::Value =
            serde_json::from_str(&json_file).context("parsing json file")?;
        let abi = serde_json::from_value(raw["abi"].clone())?;
        let bytecode_hex = raw["bytecode"].as_str().expect("bytecode");
        let bytecode = hex::decode(&bytecode_hex[2..])
            .context("decoding bytecode from hex to bytes")?;
        Ok(ContractFactory::new(abi, bytecode.into(), client))
    }
    async fn deploy_anchor_contract<M: Middleware + 'static>(
        client: Arc<M>,
    ) -> anyhow::Result<Address> {
        let hasher_factory = create_contract_factory(
            "tests/build/contracts/Hasher.json",
            client.clone(),
        )
        .context("create hasher factory")?;
        let verifier_factory = create_contract_factory(
            "tests/build/contracts/Verifier.json",
            client.clone(),
        )
        .context("create verifier factory")?;
        let native_anchor_factory = create_contract_factory(
            "tests/build/contracts/NativeAnchor.json",
            client.clone(),
        )
        .context("create native anchor factory")?;
        let hasher_instance = hasher_factory
            .deploy(())
            .context("deploy hasher")?
            .send()
            .await?;
        let verifier_instance = verifier_factory
            .deploy(())
            .context("deploy verifier")?
            .send()
            .await?;

        let verifier_address = verifier_instance.address().into_token();
        let hasher_address = hasher_instance.address().into_token();
        let denomination = ethers::utils::parse_ether("1")?.into_token();
        let merkle_tree_hight = 20u32.into_token();
        let args = (
            verifier_address,
            hasher_address,
            denomination,
            merkle_tree_hight,
        );
        let native_anchor_instance = native_anchor_factory
            .deploy(args)
            .context("deploy native anchor")?
            .send()
            .await?;
        Ok(native_anchor_instance.address())
    }

    async fn launch_ganache() -> GanacheInstance {
        tokio::task::spawn_blocking(|| Ganache::new().port(1998u16).spawn())
            .await
            .unwrap()
    }

    async fn make_deposit<M: 'static + Middleware>(
        rng: &mut impl rand::Rng,
        contract: &AnchorContract<M>,
        leaves: &mut Vec<H256>,
    ) -> anyhow::Result<()> {
        let note = Note::builder()
            .with_chain_id(1337u64)
            .with_amount(1u64)
            .with_currency("ETH")
            .build(rng);
        let deposit: Deposit = note.clone().into();
        let tx = contract
            .deposit(deposit.commitment.into())
            .value(ethers::utils::parse_ether(note.amount)?);
        let result = tx.send().await?;
        let _receipt = result.await?;
        leaves.push(deposit.commitment);
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn leaves_watcher() -> anyhow::Result<()> {
        env_logger::builder().is_test(true).init();
        let ganache = launch_ganache().await;
        let provider = Provider::<Http>::try_from(ganache.endpoint())?
            .interval(Duration::from_millis(10u64));
        let key = ganache.keys().first().cloned().unwrap();
        let wallet = LocalWallet::from(key).set_chain_id(1337u64);
        let client = SignerMiddleware::new(provider, wallet);
        let client = Arc::new(client);
        let anchor_contract_address =
            deploy_anchor_contract(client.clone()).await?;
        let contract =
            AnchorContract::new(anchor_contract_address, client.clone());
        let mut expected_leaves = Vec::new();
        let mut rng = rand::rngs::StdRng::from_seed([0u8; 32]);
        // make a couple of deposit now, before starting the watcher.
        make_deposit(&mut rng, &contract, &mut expected_leaves).await?;
        make_deposit(&mut rng, &contract, &mut expected_leaves).await?;
        let store = InMemoryLeafCache::default();
        let leaves_watcher = LeavesWatcher::new(
            ganache.ws_endpoint(),
            store.clone(),
            anchor_contract_address,
        );
        // run the leaves watcher in another task
        let task_handle = tokio::task::spawn(leaves_watcher.watch());
        // then, make another deposit, while the watcher is running.
        make_deposit(&mut rng, &contract, &mut expected_leaves).await?;
        // it should now contains the 2 leaves when the watcher was offline, and
        // the new one that happened while it is watching.
        let leaves = store.get_leaves(anchor_contract_address)?;
        assert_eq!(expected_leaves, leaves);
        // now let's abort it, and try to do another deposit.
        task_handle.abort();
        make_deposit(&mut rng, &contract, &mut expected_leaves).await?;
        // let's run it again, using the same old store.
        let leaves_watcher = LeavesWatcher::new(
            ganache.ws_endpoint(),
            store.clone(),
            anchor_contract_address,
        );
        let task_handle = tokio::task::spawn(leaves_watcher.watch());
        log::debug!("Waiting for 5s allowing the task to run..");
        // let's wait for a bit.. to allow the task to run.
        tokio::time::sleep(Duration::from_secs(5)).await;
        // now it should now contain all the old leaves + the missing one.
        let leaves = store.get_leaves(anchor_contract_address)?;
        assert_eq!(expected_leaves, leaves);
        task_handle.abort();
        Ok(())
    }
}
