use std::convert::TryFrom;
use std::sync::Arc;
use std::time::Duration;

use webb::evm::ethers::providers;

use crate::context::RelayerContext;

use crate::config::*;
use crate::events_watcher::*;

type Client = providers::Provider<providers::Http>;
type Store = crate::store::sled::SledStore;

pub fn ignite(ctx: &RelayerContext, store: Arc<Store>) -> anyhow::Result<()> {
    // now we go through each chain, in our configuration
    for (chain_name, chain_config) in &ctx.config.evm {
        let provider = Client::try_from(chain_config.http_endpoint.as_str())?
            .interval(Duration::from_millis(6u64));
        let client = Arc::new(provider);
        tracing::debug!(
            "Starting Background Services for ({}) chain.",
            chain_name
        );
        for contract in &chain_config.contracts {
            match contract {
                Contract::Anchor(config) => {
                    start_anchor_events_watcher(
                        ctx,
                        config,
                        client.clone(),
                        store.clone(),
                    )?;
                }
                Contract::Anchor2(config) => {
                    start_anchor2_events_watcher(
                        ctx,
                        config,
                        client.clone(),
                        store.clone(),
                    )?;
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
    }
    Ok(())
}

fn start_anchor_events_watcher(
    ctx: &RelayerContext,
    config: &AnchorContractConfig,
    client: Arc<Client>,
    store: Arc<Store>,
) -> anyhow::Result<()> {
    // check first if we should start the events watcher for this contract.
    if !config.events_watcher.enabled {
        tracing::warn!(
            "Anchor events watcher is disabled for ({}).",
            config.common.address,
        );
        return Ok(());
    }
    let wrapper = AnchorContractWrapper::new(config.clone(), client.clone());
    tracing::debug!(
        "Anchor events watcher for ({}) Started.",
        config.common.address,
    );
    let watcher = AnchorLeavesWatcher.run(client, store, wrapper);
    let mut shutdown_signal = ctx.shutdown_signal();
    let contract_address = config.common.address;
    let task = async move {
        tokio::select! {
            _ = watcher => {
                tracing::warn!(
                    "Anchor events watcher stopped for ({})",
                    contract_address,
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping Anchor events watcher for ({})",
                    contract_address,
                );
            },
        }
    };
    // kick off the watcher.
    tokio::task::spawn(task);
    Ok(())
}

fn start_anchor2_events_watcher(
    ctx: &RelayerContext,
    config: &Anchor2ContractConfig,
    client: Arc<Client>,
    store: Arc<Store>,
) -> anyhow::Result<()> {
    if !config.events_watcher.enabled {
        tracing::warn!(
            "Anchor2 events watcher is disabled for ({}).",
            config.common.address,
        );
        return Ok(());
    }
    let wrapper = Anchor2ContractWrapper::new(
        config.clone(),
        ctx.config.clone(), // the original config to access all networks.
        client.clone(),
    );
    tracing::debug!(
        "Anchor2 events watcher for ({}) Started.",
        config.common.address,
    );

    const LEAVES_WATCHER: Anchor2LeavesWatcher = Anchor2LeavesWatcher::new();
    let leaves_watcher_task =
        LEAVES_WATCHER.run(client.clone(), store.clone(), wrapper.clone());

    const BRIDGE_WATCHER: Anchor2BridgeWatcher = Anchor2BridgeWatcher::new();
    let bridge_watcher_task = BRIDGE_WATCHER.run(client, store, wrapper);
    let mut shutdown_signal = ctx.shutdown_signal();
    let contract_address = config.common.address;
    let task = async move {
        tokio::select! {
            _ = leaves_watcher_task => {
                tracing::warn!(
                    "Anchor2 leaves watcher task stopped for ({})",
                    contract_address,
                );
            },
            _ = bridge_watcher_task => {
                tracing::warn!(
                    "Anchor2 bridge watcher task stopped for ({})",
                    contract_address,
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping Anchor2 watcher for ({})",
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

    let events_watcher_task = EventWatcher::run(
        &BridgeContractWatcher,
        client.clone(),
        store.clone(),
        wrapper.clone(),
    );
    let cmd_handler_task =
        BridgeWatcher::run(&BridgeContractWatcher, client, store, wrapper);
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
