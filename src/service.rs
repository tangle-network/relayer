use std::sync::Arc;

use ethereum_types::U256;
use webb::evm::ethers::providers;
use webb::substrate::subxt::PairSigner;
use webb::substrate::{dkg_runtime, subxt};

use crate::config::*;
use crate::context::RelayerContext;
use crate::events_watcher::*;
use crate::tx_queue::TxQueue;

type Client = providers::Provider<providers::Http>;
type DkgClient = subxt::Client<dkg_runtime::api::DefaultConfig>;
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
        match node_config.runtime {
            SubstrateRuntime::Dkg => {
                let client = ctx
                    .substrate_provider::<dkg_runtime::api::DefaultConfig>(
                        node_name,
                    )
                    .await?;
                let metadata = client.metadata();
                let chain_identifier = metadata
                    .pallet("DKGProposals")?
                    .constant("ChainIdentifier")?;
                let mut chain_id_bytes = [0u8; 4];
                chain_id_bytes.copy_from_slice(&chain_identifier.value[0..4]);
                let chain_id = U256::from(u32::from_le_bytes(chain_id_bytes));
                // TODO(@shekohex): start the dkg service
                for pallet in &node_config.pallets {
                    match pallet {
                        Pallet::DKGProposalHandler(config) => {
                            start_dkg_proposal_handler(
                                ctx,
                                config,
                                client.clone(),
                                node_name.clone(),
                                chain_id,
                                store.clone(),
                            )?;
                        }
                        Pallet::DKGProposals(_) => {
                            // TODO(@shekohex): start the dkg proposals service
                        }
                    }
                }
            }
            SubstrateRuntime::WebbProtocol => {
                // Handle Webb Protocol here
            }
        };
    }
    Ok(())
}

fn start_dkg_proposal_handler(
    ctx: &RelayerContext,
    config: &DKGProposalHandlerPalletConfig,
    client: DkgClient,
    node_name: String,
    chain_id: U256,
    store: Arc<Store>,
) -> anyhow::Result<()> {
    // check first if we should start the events watcher for this contract.
    if !config.events_watcher.enabled {
        tracing::warn!(
            "DKG Proposal Handler events watcher is disabled for ({}).",
            node_name,
        );
        return Ok(());
    }
    tracing::debug!(
        "DKG Proposal Handler events watcher for ({}) Started.",
        node_name,
    );
    let node_name2 = node_name.clone();
    let watcher =
        ProposalHandlerWatcher.run(node_name, chain_id, client, store);
    let mut shutdown_signal = ctx.shutdown_signal();
    let task = async move {
        tokio::select! {
            _ = watcher => {
                tracing::warn!(
                    "DKG Proposal Handler events watcher stopped for ({})",
                    node_name2,
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping DKG Proposal Handler events watcher for ({})",
                    node_name2,
                );
            },
        }
    };
    // kick off the watcher.
    tokio::task::spawn(task);
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
    let mut shutdown_signal = ctx.shutdown_signal();
    let contract_address = config.common.address;
    let anchor_leaves_store = store.clone();
    let anchor_leaves_client = client.clone();
    let anchor_leaves_wrapper = wrapper.clone();
    let anchor_leaves_watcher_task = async move {
        tracing::debug!(
            "Anchor leaves events watcher for ({}) Started.",
            contract_address,
        );

        let anchor_leaves_watcher = AnchorLeavesWatcher::new();
        let leaves_watcher_task = anchor_leaves_watcher.run(
            anchor_leaves_client,
            anchor_leaves_store,
            anchor_leaves_wrapper,
        );

        tokio::select! {
            _ = leaves_watcher_task => {
                tracing::warn!(
                    "Anchor leaves watcher task stopped for ({})",
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
    let mut shutdown_signal = ctx.shutdown_signal();
    let contract_address = config.common.address;
    let anchor_bridge_watcher_task = async move {
        tracing::debug!(
            "Anchor bridge events watcher for ({}) Started.",
            contract_address,
        );

        let anchor_bridge_watcher = AnchorBridgeWatcher::new();
        let anchor_bridge_watcher_task =
            anchor_bridge_watcher.run(client, store, wrapper);

        tokio::select! {
            _ = anchor_bridge_watcher_task => {
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
    tokio::task::spawn(anchor_leaves_watcher_task);
    tokio::task::spawn(anchor_bridge_watcher_task);

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

    let dkg_client = ctx
        .substrate_provider::<dkg_runtime::api::DefaultConfig>(&config.dkg_node)
        .await?;
    let pair = ctx.substrate_wallet(&config.dkg_node).await?;
    let mut shutdown_signal = ctx.shutdown_signal();
    let contract_address = config.common.address;
    let task = async move {
        tracing::debug!(
            "Anchor Over DKG events watcher for ({}) Started.",
            contract_address,
        );
        let watcher =
            AnchorWatcherOverDKG::new(dkg_client, PairSigner::new(pair));
        let anchor_over_dkg_watcher_task = watcher.run(client, store, wrapper);
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
    let mut shutdown_signal = ctx.shutdown_signal();
    let contract_address = config.common.address;
    let task = async move {
        tracing::debug!("Bridge watcher for ({}) Started.", contract_address);
        let bridge_contract_watcher = BridgeContractWatcher::default();
        let events_watcher_task = EventWatcher::run(
            &bridge_contract_watcher,
            client.clone(),
            store.clone(),
            wrapper.clone(),
        );
        let cmd_handler_task = BridgeWatcher::run(
            &bridge_contract_watcher,
            client,
            store,
            wrapper,
        );
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
