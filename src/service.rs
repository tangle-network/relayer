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
//! # Relayer Service Module 🕸️
//!
//! A module for starting long-running tasks for event watching.
//!
//! ## Overview
//!
//! Services are tasks which the relayer constantly runs throughout its lifetime.
//! Services handle keeping up to date with the configured chains.

use std::collections::HashMap;
use std::sync::Arc;

use ethereum_types::U256;
use webb::evm::ethers::providers;
use webb::substrate::dkg_runtime::api::runtime_types::webb_proposals::header::TypedChainId;
use webb::substrate::dkg_runtime::api::RuntimeApi as DkgRuntimeApi;
use webb::substrate::subxt::DefaultConfig;
use webb::substrate::{
    protocol_substrate_runtime::api::RuntimeApi as WebbProtocolRuntimeApi,
    subxt::{self, PairSigner},
};

use crate::config::*;
use crate::context::RelayerContext;
use crate::events_watcher::dkg::*;
use crate::events_watcher::evm::*;
use crate::events_watcher::substrate::*;
use crate::events_watcher::*;
use crate::proposal_signing_backend::*;
use crate::store::sled::SledStore;
use crate::tx_queue::{evm::TxQueue, substrate::SubstrateTxQueue};

/// Type alias for providers
type Client = providers::Provider<providers::Http>;
/// Type alias for the DKG DefaultConfig
type DkgClient = subxt::Client<subxt::DefaultConfig>;
/// Type alias for the DKG RuntimeApi
type DkgRuntime = DkgRuntimeApi<
    subxt::DefaultConfig,
    subxt::PolkadotExtrinsicParams<subxt::DefaultConfig>,
>;
/// Type alias for the WebbProtocol DefaultConfig
type WebbProtocolClient = subxt::Client<subxt::DefaultConfig>;
/// Type alias for the WebbProtocol RuntimeApi
type WebbProtocolRuntime = WebbProtocolRuntimeApi<
    subxt::DefaultConfig,
    subxt::SubstrateExtrinsicParams<subxt::DefaultConfig>,
>;
/// Type alias for [Sled](https://sled.rs)-based database store
type Store = crate::store::sled::SledStore;

/// Sets up the web socket server for the relayer,  routing (endpoint queries / requests mapped to handled code) and
/// instantiates the database store. Allows clients to interact with the relayer.
///
/// Returns `Ok((addr, server))` on success, or `Err(anyhow::Error)` on failure.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `store` - [Sled](https://sled.rs)-based database store
///
/// # Examples
///
/// ```
/// let ctx = RelayerContext::new(config);
/// let store = create_store(&args).await?;
/// let (addr, server) = build_web_services(ctx.clone(), store.clone())?;
/// ```
pub fn build_web_services(
    ctx: RelayerContext,
    store: crate::store::sled::SledStore,
) -> crate::Result<(
    std::net::SocketAddr,
    impl core::future::Future<Output = ()> + 'static,
)> {
    use warp::Filter;

    let port = ctx.config.port;
    let ctx_arc = Arc::new(ctx.clone());
    let ctx_filter = warp::any().map(move || Arc::clone(&ctx_arc)).boxed();

    // the websocket server for users to submit relay transaction requests
    let ws_filter = warp::path("ws")
        .and(warp::ws())
        .and(ctx_filter.clone())
        .map(|ws: warp::ws::Ws, ctx: Arc<RelayerContext>| {
            ws.on_upgrade(|socket| async move {
                let _ = crate::handler::accept_connection(ctx.as_ref(), socket)
                    .await;
            })
        })
        .boxed();

    // get the ip of the caller.
    let proxy_addr = [127, 0, 0, 1].into();

    // First check the x-forwarded-for with 'real_ip' for reverse proxy setups
    // This code identifies the client's ip address and sends it back to them
    // TODO: PUT THE URL FOR THIS ENDPOINT HERE.
    let ip_filter = warp::path("ip")
        .and(warp::get())
        .and(warp_real_ip::real_ip(vec![proxy_addr]))
        .and_then(crate::handler::handle_ip_info)
        .or(warp::path("ip")
            .and(warp::get())
            .and(warp::addr::remote())
            .and_then(crate::handler::handle_socket_info))
        .boxed();

    // Define the handling of a request for this relayer's information (supported networks)
    // TODO: PUT THE URL FOR THIS ENDPOINT HERE.
    let info_filter = warp::path("info")
        .and(warp::get())
        .and(ctx_filter)
        .and_then(crate::handler::handle_relayer_info)
        .boxed();

    // Define the handling of a request for the leaves of a merkle tree. This is used by clients as a way to query
    // for information needed to generate zero-knowledge proofs (it is faster than querying the chain history)
    // TODO: PUT THE URL FOR THIS ENDPOINT HERE.
    let evm_store = Arc::new(store.clone());
    let store_filter = warp::any().map(move || Arc::clone(&evm_store)).boxed();
    let ctx_arc = Arc::new(ctx.clone());
    let leaves_cache_filter_evm = warp::path("leaves")
        .and(warp::path("evm"))
        .and(store_filter)
        .and(warp::path::param())
        .and(warp::path::param())
        .and_then(move |store, chain_id, contract| {
            crate::handler::handle_leaves_cache_evm(
                store,
                chain_id,
                contract,
                Arc::clone(&ctx_arc),
            )
        })
        .boxed();
    // leaf api handler for substrate
    let substrate_store = Arc::new(store);
    let store_filter = warp::any()
        .map(move || Arc::clone(&substrate_store))
        .boxed();
    let ctx_arc = Arc::new(ctx.clone());

    // TODO: PUT THE URL FOR THIS ENDPOINT HERE.
    let leaves_cache_filter_substrate = warp::path("leaves")
        .and(warp::path("substrate"))
        .and(store_filter)
        .and(warp::path::param())
        .and(warp::path::param())
        .and_then(move |store, chain_id, contract| {
            crate::handler::handle_leaves_cache_substrate(
                store,
                chain_id,
                contract,
                Arc::clone(&ctx_arc),
            )
        })
        .boxed();
    // Code that will map the request handlers above to a defined http endpoint.
    let routes = ip_filter
        .or(info_filter)
        .or(leaves_cache_filter_evm)
        .or(leaves_cache_filter_substrate)
        .boxed(); // will add more routes here.
    let http_filter =
        warp::path("api").and(warp::path("v1")).and(routes).boxed();

    let cors = warp::cors().allow_any_origin();
    let service = http_filter
        .or(ws_filter)
        .with(cors)
        .with(warp::trace::request());
    let mut shutdown_signal = ctx.shutdown_signal();
    let shutdown_signal = async move {
        shutdown_signal.recv().await;
    };
    warp::serve(service)
        .try_bind_with_graceful_shutdown(([0, 0, 0, 0], port), shutdown_signal)
        .map_err(Into::into)
}

/// Starts all background services for all chains configured in the config file.
///
/// Returns a future that resolves when all services are started successfully.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `store` -[Sled](https://sled.rs)-based database store
///
/// # Examples
///
/// ```
/// let _ = service::ignite(&ctx, Arc::new(store)).await?;
/// ```
pub async fn ignite(
    ctx: &RelayerContext,
    store: Arc<Store>,
) -> crate::Result<()> {
    tracing::debug!(
        "Relayer configuration: {}",
        serde_json::to_string_pretty(&ctx.config)?
    );

    // now we go through each chain, in our configuration
    for chain_config in ctx.config.evm.values() {
        if !chain_config.enabled {
            continue;
        }
        let chain_name = &chain_config.name;
        let chain_id = U256::from(chain_config.chain_id);
        let provider = ctx.evm_provider(&chain_id.to_string()).await?;
        let client = Arc::new(provider);
        tracing::debug!(
            "Starting Background Services for ({}) chain.",
            chain_name
        );

        for contract in &chain_config.contracts {
            match contract {
                Contract::VAnchor(config) => {
                    start_evm_vanchor_events_watcher(
                        ctx,
                        config,
                        chain_id,
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
            }
        }
        // start the transaction queue after starting other tasks.
        start_tx_queue(
            ctx.clone(),
            chain_config.chain_id.to_string().clone(),
            store.clone(),
        )?;
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
                    TypedChainId::Evm(id)
                    | TypedChainId::Substrate(id)
                    | TypedChainId::PolkadotParachain(id)
                    | TypedChainId::KusamaParachain(id)
                    | TypedChainId::RococoParachain(id)
                    | TypedChainId::Cosmos(id)
                    | TypedChainId::Solana(id) => id,
                    TypedChainId::Ink(id) => id,
                };
                let chain_id = U256::from(chain_id);
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
                        Pallet::Dkg(config) => {
                            start_dkg_pallet_watcher(
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
                        Pallet::SignatureBridge(_) => {
                            unreachable!()
                        }
                        Pallet::VAnchorBn254(_) => {
                            unreachable!()
                        }
                    }
                }
                // start the transaction queue for dkg-substrate extrinsics after starting other tasks.
                start_dkg_substrate_tx_queue(
                    ctx.clone(),
                    chain_id,
                    store.clone(),
                )?;
            }
            SubstrateRuntime::WebbProtocol => {
                let client = ctx
                    .substrate_provider::<subxt::DefaultConfig>(node_name)
                    .await?;
                let api =
                    client.clone().to_runtime_api::<WebbProtocolRuntime>();
                let chain_id =
                    api.constants().linkable_tree_bn254().chain_identifier()?;
                let chain_id = U256::from(chain_id);
                for pallet in &node_config.pallets {
                    match pallet {
                        Pallet::VAnchorBn254(config) => {
                            start_substrate_vanchor_event_watcher(
                                ctx,
                                config,
                                client.clone(),
                                node_name.clone(),
                                chain_id,
                                store.clone(),
                            )?;
                        }
                        Pallet::SignatureBridge(config) => {
                            start_substrate_signature_bridge_events_watcher(
                                ctx.clone(),
                                config,
                                client.clone(),
                                node_name.clone(),
                                chain_id,
                                store.clone(),
                            )
                            .await?;
                        }
                        Pallet::DKGProposals(_) => {
                            unreachable!()
                        }
                        Pallet::DKGProposalHandler(_) => {
                            unreachable!()
                        }
                        Pallet::Dkg(_) => {
                            unreachable!()
                        }
                    }
                }

                // start the transaction queue for protocol-substrate  after starting other tasks.
                start_protocol_substrate_tx_queue(
                    ctx.clone(),
                    chain_id,
                    store.clone(),
                )?;
            }
        };
    }
    Ok(())
}

/// Starts the event watcher for Substrate vanchor events.
///
/// Returns Ok(()) if successful, or an error if not.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `config` - VAnchorBn254 configuration
/// * `client` - WebbProtocol client
/// * `node_name` - Name of the node
/// * `chain_id` - An U256 representing the chain id of the chain
/// * `store` -[Sled](https://sled.rs)-based database store
fn start_substrate_vanchor_event_watcher(
    ctx: &RelayerContext,
    config: &VAnchorBn254PalletConfig,
    client: WebbProtocolClient,
    node_name: String,
    chain_id: U256,
    store: Arc<Store>,
) -> crate::Result<()> {
    if !config.events_watcher.enabled {
        tracing::warn!(
            "Substrate VAnchor events watcher is disabled for ({}).",
            node_name,
        );
        return Ok(());
    }
    tracing::debug!(
        "Substrate VAnchor events watcher for ({}) Started.",
        node_name,
    );

    let my_ctx = ctx.clone();
    let my_config = config.clone();
    let mut shutdown_signal = ctx.shutdown_signal();
    let task = async move {
        let watcher = SubstrateVAnchorLeavesWatcher::default();
        let substrate_leaves_watcher_task = watcher.run(
            node_name.clone(),
            chain_id,
            client.clone(),
            store.clone(),
        );
        let proposal_signing_backend = make_substrate_proposal_signing_backend(
            &my_ctx,
            store.clone(),
            chain_id,
            my_config.linked_anchors.clone(),
            my_config.proposal_signing_backend,
        )
        .await?;
        match proposal_signing_backend {
            ProposalSigningBackendSelector::Dkg(backend) => {
                // its safe to use unwrap on linked_anchors here
                // since this option is always going to return Some(value).
                // linked_anchors are validated in make_proposal_signing_backend() method
                let watcher = SubstrateVAnchorWatcher::new(
                    backend,
                    my_config.linked_anchors.unwrap(),
                );
                let substrate_vanchor_watcher_task = watcher.run(
                    node_name.clone(),
                    chain_id,
                    client.clone(),
                    store.clone(),
                );
                tokio::select! {
                    _ = substrate_vanchor_watcher_task => {
                        tracing::warn!(
                            "Substrate VAnchor watcher (DKG Backend) task stopped for ({})",
                            node_name,
                        );
                    },
                    _ = substrate_leaves_watcher_task => {
                        tracing::warn!(
                            "Substrate VAnchor leaves watcher stopped for ({})",
                            node_name,
                        );
                    },
                    _ = shutdown_signal.recv() => {
                        tracing::trace!(
                            "Stopping Substrate VAnchor watcher (DKG Backend) for ({})",
                            node_name,
                        );
                    },
                }
            }
            ProposalSigningBackendSelector::Mocked(backend) => {
                // its safe to use unwrap on linked_anchors here
                // since this option is always going to return Some(value).
                let watcher = SubstrateVAnchorWatcher::new(
                    backend,
                    my_config.linked_anchors.unwrap(),
                );
                let substrate_vanchor_watcher_task = watcher.run(
                    node_name.clone(),
                    chain_id,
                    client.clone(),
                    store.clone(),
                );
                tokio::select! {
                    _ = substrate_vanchor_watcher_task => {
                        tracing::warn!(
                            "Substrate VAnchor watcher (Mocked Backend) task stopped for ({})",
                            node_name,
                        );
                    },
                    _ = substrate_leaves_watcher_task => {
                        tracing::warn!(
                            "Substrate VAnchor leaves watcher stopped for ({})",
                            node_name,
                        );
                    },
                    _ = shutdown_signal.recv() => {
                        tracing::trace!(
                            "Stopping Substrate VAnchor watcher (Mocked Backend) for ({})",
                            node_name,
                        );
                    },
                }
            }
            ProposalSigningBackendSelector::None => {
                tokio::select! {
                    _ = substrate_leaves_watcher_task => {
                        tracing::warn!(
                            "Substrate VAnchor leaves watcher stopped for ({})",
                            node_name,
                        );
                    },
                    _ = shutdown_signal.recv() => {
                        tracing::trace!(
                            "Stopping Substrate VAnchor watcher (Mocked Backend) for ({})",
                            node_name,
                        );
                    },
                }
            }
        };
        crate::Result::Ok(())
    };
    // kick off the watcher.
    tokio::task::spawn(task);
    Ok(())
}

/// Starts the event watcher for DKG proposal handler events.
///
/// Returns Ok(()) if successful, or an error if not.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `config` - DKG proposal handler configuration
/// * `client` - DKG client
/// * `node_name` - Name of the node
/// * `chain_id` - An U256 representing the chain id of the chain
/// * `store` -[Sled](https://sled.rs)-based database store
fn start_dkg_proposal_handler(
    ctx: &RelayerContext,
    config: &DKGProposalHandlerPalletConfig,
    client: DkgClient,
    node_name: String,
    chain_id: U256,
    store: Arc<Store>,
) -> crate::Result<()> {
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

/// Starts the event watcher for DKG pallet events watcher.
///
/// Returns Ok(()) if successful, or an error if not.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `config` - DKG pallet configuration
/// * `client` - DKG client
/// * `node_name` - Name of the node
/// * `chain_id` - An U256 representing the chain id of the chain
/// * `store` -[Sled](https://sled.rs)-based database store
fn start_dkg_pallet_watcher(
    ctx: &RelayerContext,
    config: &DKGPalletConfig,
    client: DkgClient,
    node_name: String,
    chain_id: U256,
    store: Arc<Store>,
) -> crate::Result<()> {
    // check first if we should start the events watcher for this pallet.
    if !config.events_watcher.enabled {
        tracing::warn!(
            "DKG Pallet events watcher is disabled for ({}).",
            node_name,
        );
        return Ok(());
    }
    tracing::debug!("DKG Pallet events watcher for ({}) Started.", node_name,);
    let node_name2 = node_name.clone();
    let mut shutdown_signal = ctx.shutdown_signal();
    let webb_config = ctx.config.clone();
    let task = async move {
        let governor_watcher = DKGGovernorWatcher::new(webb_config);
        let watcher = governor_watcher.run(node_name, chain_id, client, store);
        tokio::select! {
            _ = watcher => {
                tracing::warn!(
                    "DKG Pallet events watcher stopped for ({})",
                    node_name2,
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping DKG Pallet events watcher for ({})",
                    node_name2,
                );
            },
        }
    };
    // kick off the watcher.
    tokio::task::spawn(task);
    Ok(())
}

/// Starts the event watcher for EVM VAnchor events.
///
/// Returns Ok(()) if successful, or an error if not.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `config` - VAnchor contract configuration
/// * `client` - EVM Chain api client
/// * `store` -[Sled](https://sled.rs)-based database store
async fn start_evm_vanchor_events_watcher(
    ctx: &RelayerContext,
    config: &VAnchorContractConfig,
    chain_id: U256,
    client: Arc<Client>,
    store: Arc<Store>,
) -> crate::Result<()> {
    if !config.events_watcher.enabled {
        tracing::warn!(
            "VAnchor events watcher is disabled for ({}).",
            config.common.address,
        );
        return Ok(());
    }
    let wrapper = VAnchorContractWrapper::new(
        config.clone(),
        ctx.config.clone(), // the original config to access all networks.
        client.clone(),
    );
    let mut shutdown_signal = ctx.shutdown_signal();
    let contract_address = config.common.address;
    let my_ctx = ctx.clone();
    let my_config = config.clone();
    let task = async move {
        tracing::debug!(
            "VAnchor events watcher for ({}) Started.",
            contract_address,
        );
        let contract_watcher = VAnchorContractWatcher::default();
        let proposal_signing_backend = make_proposal_signing_backend(
            &my_ctx,
            store.clone(),
            chain_id,
            my_config.linked_anchors,
            my_config.proposal_signing_backend,
        )
        .await?;
        match proposal_signing_backend {
            ProposalSigningBackendSelector::Dkg(backend) => {
                let deposit_handler = VAnchorDepositHandler::new(backend);
                let leaves_handler = VAnchorLeavesHandler::default();
                let vanchor_watcher_task = contract_watcher.run(
                    client,
                    store,
                    wrapper,
                    vec![Box::new(deposit_handler), Box::new(leaves_handler)],
                );
                tokio::select! {
                    _ = vanchor_watcher_task => {
                        tracing::warn!(
                            "VAnchor watcher task stopped for ({})",
                            contract_address,
                        );
                    },
                    _ = shutdown_signal.recv() => {
                        tracing::trace!(
                            "Stopping VAnchor watcher for ({})",
                            contract_address,
                        );
                    },
                }
            }
            ProposalSigningBackendSelector::Mocked(backend) => {
                let deposit_handler = VAnchorDepositHandler::new(backend);
                let leaves_handler = VAnchorLeavesHandler::default();
                let vanchor_watcher_task = contract_watcher.run(
                    client,
                    store,
                    wrapper,
                    vec![Box::new(deposit_handler), Box::new(leaves_handler)],
                );
                tokio::select! {
                    _ = vanchor_watcher_task => {
                        tracing::warn!(
                            "VAnchor watcher task stopped for ({})",
                            contract_address,
                        );
                    },
                    _ = shutdown_signal.recv() => {
                        tracing::trace!(
                            "Stopping VAnchor watcher for ({})",
                            contract_address,
                        );
                    },
                }
            }
            ProposalSigningBackendSelector::None => {
                let leaves_handler = VAnchorLeavesHandler::default();
                let vanchor_watcher_task = contract_watcher.run(
                    client,
                    store,
                    wrapper,
                    vec![Box::new(leaves_handler)],
                );
                tokio::select! {
                    _ = vanchor_watcher_task => {
                        tracing::warn!(
                            "VAnchor watcher task stopped for ({})",
                            contract_address,
                        );
                    },
                    _ = shutdown_signal.recv() => {
                        tracing::trace!(
                            "Stopping VAnchor watcher for ({})",
                            contract_address,
                        );
                    },
                }
            }
        };

        crate::Result::Ok(())
    };
    // kick off the watcher.
    tokio::task::spawn(task);
    Ok(())
}

/// Starts the event watcher for Signature Bridge contract.
async fn start_signature_bridge_events_watcher(
    ctx: &RelayerContext,
    config: &SignatureBridgeContractConfig,
    client: Arc<Client>,
    store: Arc<Store>,
) -> crate::Result<()> {
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
        tracing::debug!(
            "Signature Bridge watcher for ({}) Started.",
            contract_address
        );
        let bridge_contract_watcher = SignatureBridgeContractWatcher::default();
        let governance_transfer_handler =
            SignatureBridgeGovernanceOwnershipTransferredHandler::default();
        let events_watcher_task = EventWatcher::run(
            &bridge_contract_watcher,
            client.clone(),
            store.clone(),
            wrapper.clone(),
            vec![Box::new(governance_transfer_handler)],
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

/// Starts the event watcher for Signature Bridge Pallet.
async fn start_substrate_signature_bridge_events_watcher(
    ctx: RelayerContext,
    config: &SignatureBridgePalletConfig,
    client: WebbProtocolClient,
    node_name: String,
    chain_id: U256,
    store: Arc<Store>,
) -> crate::Result<()> {
    if !config.events_watcher.enabled {
        tracing::warn!(
            "Substrate Signature Bridge events watcher is disabled for ({}).",
            node_name,
        );
        return Ok(());
    }
    let mut shutdown_signal = ctx.shutdown_signal();
    let task = async move {
        tracing::debug!(
            "Substrate Signature Bridge watcher for ({}) Started.",
            node_name
        );
        let substrate_bridge_watcher = SubstrateBridgeEventWatcher::default();
        let events_watcher_task = SubstrateEventWatcher::run(
            &substrate_bridge_watcher,
            node_name.clone(),
            chain_id,
            client.clone(),
            store.clone(),
        );
        let cmd_handler_task = SubstrateBridgeWatcher::run(
            &substrate_bridge_watcher,
            chain_id,
            client.clone(),
            store.clone(),
        );
        tokio::select! {
            _ = events_watcher_task => {
                tracing::warn!(
                    "Substrate signature bridge events watcher task stopped for ({})",
                    node_name
                );
            },
            _ = cmd_handler_task => {
                tracing::warn!(
                    "Substrate signature bridge cmd handler task stopped for ({})",
                    node_name
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping Substrate Signature Bridge watcher for ({})",
                    node_name,
                );
            },
        }
    };
    // kick off the watcher.
    tokio::task::spawn(task);
    Ok(())
}

/// Starts the transaction queue task
///
/// Returns Ok(()) if successful, or an error if not.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `chain_name` - Name of the chain
/// * `store` -[Sled](https://sled.rs)-based database store
fn start_tx_queue(
    ctx: RelayerContext,
    chain_id: String,
    store: Arc<Store>,
) -> crate::Result<()> {
    let mut shutdown_signal = ctx.shutdown_signal();
    let tx_queue = TxQueue::new(ctx, chain_id.clone(), store);

    tracing::debug!("Transaction Queue for ({}) Started.", chain_id);
    let task = async move {
        tokio::select! {
            _ = tx_queue.run() => {
                tracing::warn!(
                    "Transaction Queue task stopped for ({})",
                    chain_id,
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping Transaction Queue for ({})",
                    chain_id,
                );
            },
        }
    };
    // kick off the tx_queue.
    tokio::task::spawn(task);
    Ok(())
}

/// Starts the transaction queue task for protocol-substrate extrinsic
///
/// Returns Ok(()) if successful, or an error if not.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `chain_name` - Name of the chain
/// * `store` -[Sled](https://sled.rs)-based database store
fn start_protocol_substrate_tx_queue(
    ctx: RelayerContext,
    chain_id: U256,
    store: Arc<Store>,
) -> crate::Result<()> {
    let mut shutdown_signal = ctx.shutdown_signal();

    let tx_queue = SubstrateTxQueue::new(ctx, chain_id, store);

    tracing::debug!(
        "Transaction Queue for Protocol-Substrate node({}) Started.",
        chain_id
    );
    let task = async move {
        tokio::select! {
            _ = tx_queue.run::<subxt::SubstrateExtrinsicParams<subxt::DefaultConfig>>() => {
                tracing::warn!(
                    "Transaction Queue task stopped for Protocol-Substrate node({})",
                    chain_id
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping Transaction Queue for Protocol-Substrate node({})",
                    chain_id
                );
            },
        }
    };
    // kick off the substrate tx_queue.
    tokio::task::spawn(task);
    Ok(())
}

/// Starts the transaction queue task for dkg-substrate extrinsics
///
/// Returns Ok(()) if successful, or an error if not.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `chain_name` - Name of the chain
/// * `store` -[Sled](https://sled.rs)-based database store
fn start_dkg_substrate_tx_queue(
    ctx: RelayerContext,
    chain_id: U256,
    store: Arc<Store>,
) -> crate::Result<()> {
    let mut shutdown_signal = ctx.shutdown_signal();

    let tx_queue = SubstrateTxQueue::new(ctx, chain_id, store);

    tracing::debug!(
        "Transaction Queue for Dkg-Substrate node({}) Started.",
        chain_id
    );
    let task = async move {
        tokio::select! {
            _ = tx_queue.run::<subxt::PolkadotExtrinsicParams<subxt::DefaultConfig>>() => {
                tracing::warn!(
                    "Transaction Queue task stopped for Dkg-Substrate node({})",
                    chain_id
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping Transaction Queue for Dkg-Substrate node({})",
                    chain_id
                );
            },
        }
    };
    // kick off the substrate tx_queue.
    tokio::task::spawn(task);
    Ok(())
}

#[allow(clippy::large_enum_variant)]
enum ProposalSigningBackendSelector {
    None,
    Mocked(MockedProposalSigningBackend<SledStore>),
    Dkg(DkgProposalSigningBackend<DkgRuntime, DefaultConfig>),
}

async fn make_proposal_signing_backend(
    ctx: &RelayerContext,
    store: Arc<Store>,
    chain_id: U256,
    linked_anchors: Option<Vec<LinkedVAnchorConfig>>,
    proposal_signing_backend: Option<ProposalSigningBackendConfig>,
) -> crate::Result<ProposalSigningBackendSelector> {
    // Check if contract is configured with governance support for the relayer.
    if !ctx.config.features.governance_relay {
        tracing::warn!("Governance relaying is not enabled for relayer");
        return Ok(ProposalSigningBackendSelector::None);
    }

    // We do this by checking if linked anchors are provided.
    let linked_anchors = match linked_anchors {
        Some(anchors) => {
            if anchors.is_empty() {
                tracing::warn!("Misconfigured Network: Linked anchors cannot be empty for governance relaying");
                return Ok(ProposalSigningBackendSelector::None);
            } else {
                anchors
            }
        }
        None => {
            tracing::warn!("Misconfigured Network: Linked anchors must be configured for governance relaying");
            return Ok(ProposalSigningBackendSelector::None);
        }
    };

    // we need to check/match on the proposal signing backend configured for this anchor.
    match proposal_signing_backend {
        Some(ProposalSigningBackendConfig::DkgNode(c)) => {
            // if it is the dkg backend, we will need to connect to that node first,
            // and then use the DkgProposalSigningBackend to sign the proposal.
            let typed_chain_id =
                webb_proposals::TypedChainId::Evm(chain_id.as_u32());
            let dkg_client = ctx
                .substrate_provider::<subxt::DefaultConfig>(&c.node)
                .await?;
            let pair = ctx.substrate_wallet(&c.node).await?;
            let backend = DkgProposalSigningBackend::new(
                dkg_client,
                PairSigner::new(pair),
                typed_chain_id,
            );
            Ok(ProposalSigningBackendSelector::Dkg(backend))
        }
        Some(ProposalSigningBackendConfig::Mocked(mocked)) => {
            // if it is the mocked backend, we will use the MockedProposalSigningBackend to sign the proposal.
            // which is a bit simpler than the DkgProposalSigningBackend.
            // get only the linked chains to that anchor.
            let linked_chains = linked_anchors
                .iter()
                .flat_map(|c| ctx.config.evm.get(&c.chain_id.to_string()));
            // then will have to go through our configruation to retrieve the correct
            // signature bridges that are configured on the linked chains.
            // Note: this assumes that every network will only have one signature bridge configured for it.
            let signature_bridges = linked_chains
                .flat_map(|chain_config| {
                    // find the first signature bridge configured on that chain.
                    chain_config
                        .contracts
                        .iter()
                        .find(|contract| {
                            matches!(contract, Contract::SignatureBridge(_))
                        })
                        .map(|contract| (contract, chain_config.chain_id))
                })
                .map(|(bridge_contract, chain_id)| match bridge_contract {
                    Contract::SignatureBridge(c) => (c, chain_id),
                    _ => unreachable!(),
                })
                .flat_map(|(bridge_config, chain_id)| {
                    // then we just create the signature bridge metadata.
                    let chain_id =
                        webb_proposals::TypedChainId::Evm(chain_id as u32);
                    let target_system =
                        webb_proposals::TargetSystem::new_contract_address(
                            bridge_config.common.address,
                        );
                    Some((chain_id, target_system))
                })
                .collect::<HashMap<_, _>>();
            let backend = MockedProposalSigningBackend::builder()
                .store(store.clone())
                .private_key(mocked.private_key)
                .signature_bridges(signature_bridges)
                .build();
            Ok(ProposalSigningBackendSelector::Mocked(backend))
        }
        None => {
            tracing::warn!("Misconfigured Network: Proposal signing backend must be configured for governance relaying");
            Ok(ProposalSigningBackendSelector::None)
        }
    }
}

async fn make_substrate_proposal_signing_backend(
    ctx: &RelayerContext,
    store: Arc<Store>,
    chain_id: U256,
    linked_anchors: Option<Vec<SubstrateLinkedVAnchorConfig>>,
    proposal_signing_backend: Option<ProposalSigningBackendConfig>,
) -> crate::Result<ProposalSigningBackendSelector> {
    // Check if contract is configured with governance support for the relayer.
    if !ctx.config.features.governance_relay {
        tracing::warn!("Governance relaying is not enabled for relayer");
        return Ok(ProposalSigningBackendSelector::None);
    }
    // check if linked anchors are provided.
    let linked_anchors = match linked_anchors {
        Some(anchors) => {
            if anchors.is_empty() {
                tracing::warn!("Misconfigured Network: Linked anchors cannot be empty for governance relaying");
                return Ok(ProposalSigningBackendSelector::None);
            } else {
                anchors
            }
        }
        None => {
            tracing::warn!("Misconfigured Network: Linked anchors must be configured for governance relaying");
            return Ok(ProposalSigningBackendSelector::None);
        }
    };
    // we need to check/match on the proposal signing backend configured for this anchor.
    match proposal_signing_backend {
        Some(ProposalSigningBackendConfig::DkgNode(c)) => {
            // if it is the dkg backend, we will need to connect to that node first,
            // and then use the DkgProposalSigningBackend to sign the proposal.
            let typed_chain_id =
                webb_proposals::TypedChainId::Substrate(chain_id.as_u32());
            let dkg_client = ctx
                .substrate_provider::<subxt::DefaultConfig>(&c.node)
                .await?;
            let pair = ctx.substrate_wallet(&c.node).await?;
            let backend = DkgProposalSigningBackend::new(
                dkg_client,
                PairSigner::new(pair),
                typed_chain_id,
            );
            Ok(ProposalSigningBackendSelector::Dkg(backend))
        }
        Some(ProposalSigningBackendConfig::Mocked(mocked)) => {
            // if it is the mocked backend, we will use the MockedProposalSigningBackend to sign the proposal.
            // which is a bit simpler than the DkgProposalSigningBackend.
            // get only the linked chains to that anchor.

            let linked_chains = linked_anchors
                .iter()
                .flat_map(|anchor| {
                    // using chain_id to ensure that we have only one signature bridge
                    let target_system =
                        webb_proposals::TargetSystem::new_tree_id(
                            chain_id.as_u32(),
                        );
                    let chain_id =
                        webb_proposals::TypedChainId::Substrate(anchor.chain);
                    Some((chain_id, target_system))
                })
                .collect::<HashMap<_, _>>();

            let backend = MockedProposalSigningBackend::builder()
                .store(store.clone())
                .private_key(mocked.private_key)
                .signature_bridges(linked_chains)
                .build();
            Ok(ProposalSigningBackendSelector::Mocked(backend))
        }
        None => {
            tracing::warn!("Misconfigured Network: Proposal signing backend must be configured for governance relaying");
            Ok(ProposalSigningBackendSelector::None)
        }
    }
}
