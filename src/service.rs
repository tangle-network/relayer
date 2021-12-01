use std::sync::Arc;

use once_cell::sync::OnceCell;
use webb::evm::ethers::providers;
use webb::substrate::dkg_runtime;
use webb::substrate::subxt::PairSigner;

use crate::config::*;
use crate::context::RelayerContext;
use crate::events_watcher::*;
use crate::tx_queue::TxQueue;

type Client = providers::Provider<providers::Http>;
type Store = crate::store::sled::SledStore;

pub async fn ignite(
    ctx: &RelayerContext,
    store: Arc<Store>,
) -> anyhow::Result<()> {
    // now we go through each chain, in our configuration
    for (chain_name, chain_config) in &ctx.config.evm {
        if !chain_config.enabled {
            continue;
        }
        let provider = ctx.evm_provider(chain_name).await?;
        let client = Arc::new(provider);
        tracing::debug!(
            "Starting Background Services for ({}) chain.",
            chain_name
        );

        for contract in &chain_config.contracts {
            match contract {
                Contract::Tornado(config) => {
                    start_tornado_events_watcher(
                        ctx,
                        config,
                        client.clone(),
                        store.clone(),
                    )?;
                }
                Contract::Anchor(config) => {
                    start_anchor_events_watcher(
                        ctx,
                        config,
                        client.clone(),
                        store.clone(),
                    )?;
                }
                Contract::AnchorOverDKG(config) => {
                    start_anchor_over_dkg_events_watcher(
                        ctx,
                        config,
                        client.clone(),
                        store.clone(),
                    )
                    .await?;
                }
                Contract::Bridge(config) => {
                    start_bridge_watcher(
                        ctx,
                        config,
                        client.clone(),
                        store.clone(),
                    )?;
                }
                Contract::GovernanceBravoDelegate(_) => {}
            }
        }
        // start the transaction queue after starting other tasks.
        start_tx_queue(ctx.clone(), chain_name.clone(), store.clone())?;
    }
    // now, we start substrate service/tasks
    for (node_name, node_config) in &ctx.config.substrate {
        if !node_config.enabled {
            continue;
        }
        if let SubstrateRuntime::Dkg = node_config.runtime {
            let _client = ctx
                .substrate_provider::<dkg_runtime::api::DefaultConfig>(
                    node_name,
                )
                .await?;
            // TODO(@shekohex): start the dkg service
        };
    }
    Ok(())
}

fn start_tornado_events_watcher(
    ctx: &RelayerContext,
    config: &TornadoContractConfig,
    client: Arc<Client>,
    store: Arc<Store>,
) -> anyhow::Result<()> {
    // check first if we should start the events watcher for this contract.
    if !config.events_watcher.enabled {
        tracing::warn!(
            "Tornado events watcher is disabled for ({}).",
            config.common.address,
        );
        return Ok(());
    }
    let wrapper = TornadoContractWrapper::new(config.clone(), client.clone());
    tracing::debug!(
        "Tornado events watcher for ({}) Started.",
        config.common.address,
    );
    let watcher = TornadoLeavesWatcher.run(client, store, wrapper);
    let mut shutdown_signal = ctx.shutdown_signal();
    let contract_address = config.common.address;
    let task = async move {
        tokio::select! {
            _ = watcher => {
                tracing::warn!(
                    "Tornado events watcher stopped for ({})",
                    contract_address,
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping Tornado events watcher for ({})",
                    contract_address,
                );
            },
        }
    };
    // kick off the watcher.
    tokio::task::spawn(task);
    Ok(())
}

fn start_anchor_events_watcher(
    ctx: &RelayerContext,
    config: &AnchorContractConfig,
    client: Arc<Client>,
    store: Arc<Store>,
) -> anyhow::Result<()> {
    if !config.events_watcher.enabled {
        tracing::warn!(
            "Anchor events watcher is disabled for ({}).",
            config.common.address,
        );
        return Ok(());
    }
    let wrapper = AnchorContractWrapper::new(
        config.clone(),
        ctx.config.clone(), // the original config to access all networks.
        client.clone(),
    );
    tracing::debug!(
        "Anchor events watcher for ({}) Started.",
        config.common.address,
    );

    static LEAVES_WATCHER: AnchorLeavesWatcher = AnchorLeavesWatcher::new();
    let leaves_watcher_task = EventWatcher::run(
        &LEAVES_WATCHER,
        client.clone(),
        store.clone(),
        wrapper.clone(),
    );

    static BRIDGE_WATCHER: AnchorBridgeWatcher = AnchorBridgeWatcher::new();
    let bridge_watcher_task =
        EventWatcher::run(&BRIDGE_WATCHER, client, store, wrapper);
    let mut shutdown_signal = ctx.shutdown_signal();
    let contract_address = config.common.address;
    let task = async move {
        tokio::select! {
            _ = leaves_watcher_task => {
                tracing::warn!(
                    "Anchor leaves watcher task stopped for ({})",
                    contract_address,
                );
            },
            _ = bridge_watcher_task => {
                tracing::warn!(
                    "Anchor bridge watcher task stopped for ({})",
                    contract_address,
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping Anchor watcher for ({})",
                    contract_address,
                );
            },
        }
    };
    // kick off the watcher.
    tokio::task::spawn(task);

    Ok(())
}

async fn start_anchor_over_dkg_events_watcher(
    ctx: &RelayerContext,
    config: &AnchorContractOverDKGConfig,
    client: Arc<Client>,
    store: Arc<Store>,
) -> anyhow::Result<()> {
    if !config.events_watcher.enabled {
        tracing::warn!(
            "Anchor Over DKG events watcher is disabled for ({}).",
            config.common.address,
        );
        return Ok(());
    }
    let wrapper = AnchorContractOverDKGWrapper::new(
        config.clone(),
        ctx.config.clone(), // the original config to access all networks.
        client.clone(),
    );
    tracing::debug!(
        "Anchor Over DKG events watcher for ({}) Started.",
        config.common.address,
    );
    let dkg_client = ctx
        .substrate_provider::<dkg_runtime::api::DefaultConfig>(&config.dkg_node)
        .await?;
    let pair = ctx.substrate_wallet(&config.dkg_node).await?;

    static WATCHER: OnceCell<AnchorWatcherOverDKG> = OnceCell::new();
    let watcher = WATCHER.get_or_init(|| {
        AnchorWatcherOverDKG::new(dkg_client, PairSigner::new(pair))
    });
    let anchor_over_dkg_watcher_task = watcher.run(client, store, wrapper);
    let mut shutdown_signal = ctx.shutdown_signal();
    let contract_address = config.common.address;
    let task = async move {
        tokio::select! {
            _ = anchor_over_dkg_watcher_task => {
                tracing::warn!(
                    "Anchor over dkg watcher task stopped for ({})",
                    contract_address,
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping Anchor watcher for ({})",
                    contract_address,
                );
            },
        }
    };
    // kick off the watcher.
    tokio::task::spawn(task);

    Ok(())
}

fn start_bridge_watcher(
    ctx: &RelayerContext,
    config: &BridgeContractConfig,
    client: Arc<Client>,
    store: Arc<Store>,
) -> anyhow::Result<()> {
    if !config.events_watcher.enabled {
        tracing::warn!(
            "Bridge events watcher is disabled for ({}).",
            config.common.address,
        );
        return Ok(());
    }
    let wrapper = BridgeContractWrapper::new(config.clone(), client.clone());
    tracing::debug!("Bridge watcher for ({}) Started.", config.common.address,);
    static WATCHER: BridgeContractWatcher = BridgeContractWatcher;
    let events_watcher_task = EventWatcher::run(
        &WATCHER,
        client.clone(),
        store.clone(),
        wrapper.clone(),
    );
    let cmd_handler_task = BridgeWatcher::run(&WATCHER, client, store, wrapper);
    let mut shutdown_signal = ctx.shutdown_signal();
    let contract_address = config.common.address;
    let task = async move {
        tokio::select! {
            _ = events_watcher_task => {
                tracing::warn!(
                    "bridge events watcher task stopped for ({})",
                    contract_address
                );
            },
            _ = cmd_handler_task => {
                tracing::warn!(
                    "bridge cmd handler task stopped for ({})",
                    contract_address
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping Bridge watcher for ({})",
                    contract_address,
                );
            },
        }
    };
    // kick off the watcher.
    tokio::task::spawn(task);
    Ok(())
}

fn start_tx_queue(
    ctx: RelayerContext,
    chain_name: String,
    store: Arc<Store>,
) -> anyhow::Result<()> {
    let mut shutdown_signal = ctx.shutdown_signal();
    let tx_queue = TxQueue::new(ctx, chain_name.clone(), store);

    tracing::debug!("Transaction Queue for ({}) Started.", chain_name);
    let task = async move {
        tokio::select! {
            _ = tx_queue.run() => {
                tracing::warn!(
                    "Transaction Queue task stopped for ({})",
                    chain_name,
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping Transaction Queue for ({})",
                    chain_name,
                );
            },
        }
    };
    // kick off the tx_queue.
    tokio::task::spawn(task);
    Ok(())
}
