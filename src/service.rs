use std::sync::Arc;

use ethereum_types::U256;
use webb::evm::ethers::providers;
use webb::substrate::dkg_runtime::api::runtime_types::webb_proposals::header::TypedChainId;
use webb::substrate::dkg_runtime::api::RuntimeApi as DkgRuntimeApi;
use webb::substrate::subxt;
use webb::substrate::subxt::PairSigner;

use crate::config::*;
use crate::context::RelayerContext;
use crate::events_watcher::*;
use crate::tx_queue::TxQueue;

type Client = providers::Provider<providers::Http>;
type DkgClient = subxt::Client<subxt::DefaultConfig>;
type DkgRuntime = DkgRuntimeApi<
    subxt::DefaultConfig,
    subxt::DefaultExtra<subxt::DefaultConfig>,
>;
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
                Contract::AnchorOverDKG(config) => {
                    start_anchor_over_dkg_events_watcher(
                        ctx,
                        config,
                        client.clone(),
                        store.clone(),
                    )
                    .await?;
                }
                Contract::SignatureBridge(config) => {
                    start_signature_bridge_events_watcher(
                        ctx,
                        config,
                        client.clone(),
                        store.clone(),
                    )
                    .await?;
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
                    .substrate_provider::<subxt::DefaultConfig>(node_name)
                    .await?;
                let api = client.clone().to_runtime_api::<DkgRuntime>();
                let chain_id =
                    api.constants().dkg_proposals().chain_identifier()?;
                let chain_id = match chain_id {
                    TypedChainId::None => 0,
                    TypedChainId::Evm(id) => id,
                    TypedChainId::Substrate(id) => id,
                    TypedChainId::PolkadotParachain(id) => id,
                    TypedChainId::KusamaParachain(id) => id,
                    TypedChainId::RococoParachain(id) => id,
                    TypedChainId::Cosmos(id) => id,
                    TypedChainId::Solana(id) => id,
                };
                let chain_id = U256::from(chain_id);
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
    let mut shutdown_signal = ctx.shutdown_signal();
    let webb_config = ctx.config.clone();
    let task = async move {
        let proposal_handler = ProposalHandlerWatcher::new(webb_config);
        let watcher = proposal_handler.run(node_name, chain_id, client, store);
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
        .substrate_provider::<subxt::DefaultConfig>(&config.dkg_node)
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
        let leaves_watcher = AnchorLeavesWatcher::default();
        let anchor_leaves_watcher =
            leaves_watcher.run(client.clone(), store.clone(), wrapper.clone());
        let anchor_over_dkg_watcher_task = watcher.run(client, store, wrapper);
        tokio::select! {
            _ = anchor_over_dkg_watcher_task => {
                tracing::warn!(
                    "Anchor over dkg watcher task stopped for ({})",
                    contract_address,
                );
            },
            _ = anchor_leaves_watcher => {
                tracing::warn!(
                    "Anchor leaves watcher stopped for ({})",
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

async fn start_signature_bridge_events_watcher(
    ctx: &RelayerContext,
    config: &SignatureBridgeContractConfig,
    client: Arc<Client>,
    store: Arc<Store>,
) -> anyhow::Result<()> {
    if !config.events_watcher.enabled {
        tracing::warn!(
            "Signature Bridge events watcher is disabled for ({}).",
            config.common.address,
        );
        return Ok(());
    }
    let mut shutdown_signal = ctx.shutdown_signal();
    let contract_address = config.common.address;
    let wrapper =
        SignatureBridgeContractWrapper::new(config.clone(), client.clone());
    let task = async move {
        tracing::debug!("Bridge watcher for ({}) Started.", contract_address);
        let bridge_contract_watcher = SignatureBridgeContractWatcher::default();
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
                    "signature bridge events watcher task stopped for ({})",
                    contract_address
                );
            },
            _ = cmd_handler_task => {
                tracing::warn!(
                    "signature bridge cmd handler task stopped for ({})",
                    contract_address
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping Signature Bridge watcher for ({})",
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
