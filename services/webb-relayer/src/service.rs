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

//! # Relayer Service Module 🕸️
//!
//! A module for starting long-running tasks for event watching.
//!
//! ## Overview
//!
//! Services are tasks which the relayer constantly runs throughout its lifetime.
//! Services handle keeping up to date with the configured chains.

use axum::routing::get;
use axum::Router;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use webb::evm::ethers::providers;

use webb::substrate::subxt::config::{PolkadotConfig, SubstrateConfig};
use webb::substrate::subxt::{tx::PairSigner, OnlineClient};
use webb_event_watcher_traits::evm::{BridgeWatcher, EventWatcher};
use webb_event_watcher_traits::substrate::SubstrateBridgeWatcher;
use webb_event_watcher_traits::SubstrateEventWatcher;
use webb_ew_dkg::{DKGGovernorWatcher, ProposalHandlerWatcher};
use webb_ew_evm::open_vanchor::{
    OpenVAnchorDepositHandler, OpenVAnchorLeavesHandler,
};
use webb_ew_evm::signature_bridge_watcher::{
    SignatureBridgeContractWatcher, SignatureBridgeContractWrapper,
    SignatureBridgeGovernanceOwnershipTransferredHandler,
};
use webb_ew_evm::{
    vanchor::*, OpenVAnchorContractWatcher, OpenVAnchorContractWrapper,
    VAnchorContractWatcher, VAnchorContractWrapper,
};
use webb_ew_substrate::{
    SubstrateBridgeEventWatcher, SubstrateVAnchorLeavesWatcher,
    SubstrateVAnchorWatcher,
};
use webb_relayer_config::anchor::LinkedAnchorConfig;
use webb_relayer_config::evm::{
    Contract, SignatureBridgeContractConfig, VAnchorContractConfig,
};
use webb_relayer_config::signing_backend::ProposalSigningBackendConfig;
use webb_relayer_config::substrate::{
    DKGPalletConfig, DKGProposalHandlerPalletConfig, Pallet,
    SignatureBridgePalletConfig, SubstrateRuntime, VAnchorBn254PalletConfig,
};

use webb_ew_evm::vanchor::vanchor_encrypted_outputs_handler::VAnchorEncryptedOutputHandler;
use webb_proposal_signing_backends::*;
use webb_relayer_context::RelayerContext;
use webb_relayer_handlers::{handle_fee_info, handle_socket_info};

use webb_relayer_handlers::routes::info::handle_relayer_info;
use webb_relayer_handlers::routes::{encrypted_outputs, leaves, metric};
use webb_relayer_store::SledStore;
use webb_relayer_tx_queue::{evm::TxQueue, substrate::SubstrateTxQueue};

/// Type alias for providers
pub type Client = providers::Provider<providers::Http>;
/// Type alias for the DKG DefaultConfig
pub type DkgClient = OnlineClient<PolkadotConfig>;
/// Type alias for the WebbProtocol DefaultConfig
pub type WebbProtocolClient = OnlineClient<SubstrateConfig>;
/// Type alias for [Sled](https://sled.rs)-based database store
pub type Store = webb_relayer_store::sled::SledStore;

pub async fn build_axum_services(ctx: RelayerContext) -> crate::Result<()> {
    let api = Router::new()
        .route("/ip", get(handle_socket_info))
        .route("/info", get(handle_relayer_info))
        .with_state(Arc::new(ctx.clone()));

    let app = Router::new()
        .nest("/api/v1", api)
        .into_make_service_with_connect_info::<SocketAddr>();
    // TODO: run one port higher for now so it works in parallel with warp
    let socket_addr =
        SocketAddr::new("0.0.0.0".parse().unwrap(), ctx.config.port + 1);
    axum::Server::bind(&socket_addr).serve(app).await?;
    Ok(())
}

/// Sets up the web socket server for the relayer,  routing (endpoint queries / requests mapped to handled code) and
/// instantiates the database store. Allows clients to interact with the relayer.
///
/// Returns `Ok((addr, server))` on success, or `Err(anyhow::Error)` on failure.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `store` - [Sled](https://sled.rs)-based database store
pub fn build_web_services(
    ctx: RelayerContext,
    store: webb_relayer_store::sled::SledStore,
) -> crate::Result<(
    std::net::SocketAddr,
    impl core::future::Future<Output = ()> + 'static,
)> {
    use warp::Filter;

    let port = ctx.config.port;
    let ctx_arc = Arc::new(ctx.clone());
    let ctx_filter = warp::any().map(move || Arc::clone(&ctx_arc)).boxed();
    let evm_store = Arc::new(store);
    let store_filter = warp::any().map(move || Arc::clone(&evm_store)).boxed();

    // the websocket server for users to submit relay transaction requests
    let ws_filter = warp::path("ws")
        .and(warp::ws())
        .and(ctx_filter.clone())
        .map(|ws: warp::ws::Ws, ctx: Arc<RelayerContext>| {
            ws.on_upgrade(|socket| async move {
                let _ = webb_relayer_handlers::accept_connection(
                    ctx.as_ref(),
                    socket,
                )
                .await;
            })
        })
        .boxed();

    // Define the handling of a request for the leaves of a merkle tree. This is used by clients as a way to query
    // for information needed to generate zero-knowledge proofs (it is faster than querying the chain history)
    // TODO: PUT THE URL FOR THIS ENDPOINT HERE.
    let leaves_cache_filter_evm = warp::path("leaves")
        .and(warp::path("evm"))
        .and(store_filter.clone())
        .and(warp::path::param())
        .and(warp::path::param())
        .and(warp::query())
        .and(ctx_filter.clone())
        .and_then(leaves::handle_leaves_cache_evm)
        .boxed();

    // leaf api handler for substrate
    // TODO: PUT THE URL FOR THIS ENDPOINT HERE.
    let leaves_cache_filter_substrate = warp::path("leaves")
        .and(warp::path("substrate"))
        .and(store_filter.clone())
        .and(warp::path::param())
        .and(warp::path::param())
        .and(warp::path::param())
        .and(warp::query())
        .and(ctx_filter.clone())
        .and_then(leaves::handle_leaves_cache_substrate)
        .boxed();

    let encrypted_output_cache_filter_evm = warp::path("encrypted_outputs")
        .and(warp::path("evm"))
        .and(store_filter)
        .and(warp::path::param())
        .and(warp::path::param())
        .and(warp::query())
        .and(ctx_filter.clone())
        .and_then(encrypted_outputs::handle_encrypted_outputs_cache_evm)
        .boxed();

    let relayer_metrics_info = warp::path("metrics")
        .and(warp::get())
        .and_then(metric::handle_metric_info)
        .boxed();

    //  Relayer metric for particular evm resource
    let relayer_metrics_info_evm = warp::path("metrics")
        .and(warp::path("evm"))
        .and(warp::path::param())
        .and(warp::path::param())
        .and(ctx_filter.clone())
        .and_then(metric::handle_evm_metric_info)
        .boxed();

    //  Relayer metric for particular substrate resource
    let relayer_metrics_info_substrate = warp::path("metrics")
        .and(warp::path("substrate"))
        .and(warp::path::param())
        .and(warp::path::param())
        .and(warp::path::param())
        .and(ctx_filter.clone())
        .and_then(metric::handle_substrate_metric_info)
        .boxed();

    //  Information about relayer fees
    let relayer_fee_info = warp::path("fee_info")
        .and(ctx_filter)
        .and(warp::path::param())
        .and(warp::path::param())
        .and(warp::path::param())
        .and_then(handle_fee_info)
        .boxed();

    // Code that will map the request handlers above to a defined http endpoint.
    let routes = leaves_cache_filter_evm
        .or(leaves_cache_filter_substrate)
        .or(encrypted_output_cache_filter_evm)
        .or(relayer_metrics_info_evm)
        .or(relayer_metrics_info_substrate)
        .or(relayer_metrics_info)
        .or(relayer_fee_info)
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
        let chain_id = chain_config.chain_id;
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
                Contract::OpenVAnchor(config) => {
                    start_evm_open_vanchor_events_watcher(
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
        let chain_id = node_config.chain_id;
        match node_config.runtime {
            SubstrateRuntime::Dkg => {
                let client =
                    ctx.substrate_provider::<PolkadotConfig>(node_name).await?;

                let chain_id = chain_id;
                for pallet in &node_config.pallets {
                    match pallet {
                        Pallet::DKGProposalHandler(config) => {
                            start_dkg_proposal_handler(
                                ctx,
                                config,
                                client.clone(),
                                node_name.to_owned(),
                                chain_id,
                                store.clone(),
                            )?;
                        }
                        Pallet::Dkg(config) => {
                            start_dkg_pallet_watcher(
                                ctx,
                                config,
                                client.clone(),
                                node_name.to_owned(),
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
                    .substrate_provider::<SubstrateConfig>(node_name)
                    .await?;
                let chain_id = chain_id;
                for pallet in &node_config.pallets {
                    match pallet {
                        Pallet::VAnchorBn254(config) => {
                            start_substrate_vanchor_event_watcher(
                                ctx,
                                config,
                                client.clone(),
                                node_name.to_owned(),
                                chain_id,
                                store.clone(),
                            )?;
                        }
                        Pallet::SignatureBridge(config) => {
                            start_substrate_signature_bridge_events_watcher(
                                ctx.clone(),
                                config,
                                client.clone(),
                                node_name.to_owned(),
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
/// * `chain_id` - An u32 representing the chain id of the chain
/// * `store` -[Sled](https://sled.rs)-based database store
pub fn start_substrate_vanchor_event_watcher(
    ctx: &RelayerContext,
    config: &VAnchorBn254PalletConfig,
    client: WebbProtocolClient,
    node_name: String,
    chain_id: u32,
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
    let metrics = ctx.metrics.clone();
    let task = async move {
        let watcher = SubstrateVAnchorLeavesWatcher::default();
        let substrate_leaves_watcher_task = watcher.run(
            node_name.to_owned(),
            chain_id,
            client.clone().into(),
            store.clone(),
            metrics.clone(),
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
                    node_name.to_owned(),
                    chain_id,
                    client.clone().into(),
                    store.clone(),
                    metrics.clone(),
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
                    node_name.to_owned(),
                    chain_id,
                    client.into(),
                    store.clone(),
                    metrics.clone(),
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
/// * `chain_id` - An u32 representing the chain id of the chain
/// * `store` -[Sled](https://sled.rs)-based database store
pub fn start_dkg_proposal_handler(
    ctx: &RelayerContext,
    config: &DKGProposalHandlerPalletConfig,
    client: DkgClient,
    node_name: String,
    chain_id: u32,
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
    let metrics = ctx.metrics.clone();
    let task = async move {
        let proposal_handler = ProposalHandlerWatcher::default();
        let watcher = proposal_handler.run(
            node_name,
            chain_id,
            client.into(),
            store,
            metrics,
        );
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
/// * `chain_id` - An u32 representing the chain id of the chain
/// * `store` -[Sled](https://sled.rs)-based database store
pub fn start_dkg_pallet_watcher(
    ctx: &RelayerContext,
    config: &DKGPalletConfig,
    client: DkgClient,
    node_name: String,
    chain_id: u32,
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
    let metrics = ctx.metrics.clone();
    let task = async move {
        let governor_watcher = DKGGovernorWatcher::new(webb_config);
        let watcher = governor_watcher.run(
            node_name,
            chain_id,
            client.into(),
            store,
            metrics,
        );
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
    chain_id: u32,
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
                let encrypted_output_handler =
                    VAnchorEncryptedOutputHandler::default();
                let vanchor_watcher_task = contract_watcher.run(
                    client,
                    store,
                    wrapper,
                    vec![
                        Box::new(deposit_handler),
                        Box::new(leaves_handler),
                        Box::new(encrypted_output_handler),
                    ],
                    &my_ctx,
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
                let encrypted_output_handler =
                    VAnchorEncryptedOutputHandler::default();
                let vanchor_watcher_task = contract_watcher.run(
                    client,
                    store,
                    wrapper,
                    vec![
                        Box::new(deposit_handler),
                        Box::new(leaves_handler),
                        Box::new(encrypted_output_handler),
                    ],
                    &my_ctx,
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
                let encrypted_output_handler =
                    VAnchorEncryptedOutputHandler::default();
                let vanchor_watcher_task = contract_watcher.run(
                    client,
                    store,
                    wrapper,
                    vec![
                        Box::new(leaves_handler),
                        Box::new(encrypted_output_handler),
                    ],
                    &my_ctx,
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
pub async fn start_evm_open_vanchor_events_watcher(
    ctx: &RelayerContext,
    config: &VAnchorContractConfig,
    chain_id: u32,
    client: Arc<Client>,
    store: Arc<Store>,
) -> crate::Result<()> {
    if !config.events_watcher.enabled {
        tracing::warn!(
            "Open VAnchor events watcher is disabled for ({}).",
            config.common.address,
        );
        return Ok(());
    }
    let wrapper = OpenVAnchorContractWrapper::new(
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
            "Open VAnchor events watcher for ({}) Started.",
            contract_address,
        );
        let contract_watcher = OpenVAnchorContractWatcher::default();
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
                let deposit_handler = OpenVAnchorDepositHandler::new(backend);
                let leaves_handler = OpenVAnchorLeavesHandler::default();

                let vanchor_watcher_task = contract_watcher.run(
                    client,
                    store,
                    wrapper,
                    vec![Box::new(deposit_handler), Box::new(leaves_handler)],
                    &my_ctx,
                );
                tokio::select! {
                    _ = vanchor_watcher_task => {
                        tracing::warn!(
                            "Open VAnchor watcher task stopped for ({})",
                            contract_address,
                        );
                    },
                    _ = shutdown_signal.recv() => {
                        tracing::trace!(
                            "Stopping Open VAnchor watcher for ({})",
                            contract_address,
                        );
                    },
                }
            }
            ProposalSigningBackendSelector::Mocked(backend) => {
                let deposit_handler = OpenVAnchorDepositHandler::new(backend);
                let leaves_handler = OpenVAnchorLeavesHandler::default();
                let vanchor_watcher_task = contract_watcher.run(
                    client,
                    store,
                    wrapper,
                    vec![Box::new(deposit_handler), Box::new(leaves_handler)],
                    &my_ctx,
                );
                tokio::select! {
                    _ = vanchor_watcher_task => {
                        tracing::warn!(
                            "Open VAnchor watcher task stopped for ({})",
                            contract_address,
                        );
                    },
                    _ = shutdown_signal.recv() => {
                        tracing::trace!(
                            "Stopping Open VAnchor watcher for ({})",
                            contract_address,
                        );
                    },
                }
            }
            ProposalSigningBackendSelector::None => {
                let leaves_handler = OpenVAnchorLeavesHandler::default();
                let vanchor_watcher_task = contract_watcher.run(
                    client,
                    store,
                    wrapper,
                    vec![Box::new(leaves_handler)],
                    &my_ctx,
                );
                tokio::select! {
                    _ = vanchor_watcher_task => {
                        tracing::warn!(
                            "Open VAnchor watcher task stopped for ({})",
                            contract_address,
                        );
                    },
                    _ = shutdown_signal.recv() => {
                        tracing::trace!(
                            "Stopping Open VAnchor watcher for ({})",
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
pub async fn start_signature_bridge_events_watcher(
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
    let metrics = ctx.metrics.clone();
    let my_ctx = ctx.clone();
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
            &my_ctx,
        );
        let cmd_handler_task = BridgeWatcher::run(
            &bridge_contract_watcher,
            client,
            store,
            wrapper,
            metrics.clone(),
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
pub async fn start_substrate_signature_bridge_events_watcher(
    ctx: RelayerContext,
    config: &SignatureBridgePalletConfig,
    client: WebbProtocolClient,
    node_name: String,
    chain_id: u32,
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
            node_name.to_owned(),
            chain_id,
            client.clone().into(),
            store.clone(),
            ctx.metrics.clone(),
        );
        let cmd_handler_task = SubstrateBridgeWatcher::run(
            &substrate_bridge_watcher,
            chain_id,
            client.into(),
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
pub fn start_tx_queue(
    ctx: RelayerContext,
    chain_id: String,
    store: Arc<Store>,
) -> crate::Result<()> {
    // Start tx_queue only when governance relaying feature is enabled for relayer.
    if !ctx.config.features.governance_relay {
        tracing::warn!("Tx Queue disabled for ({})", chain_id,);
        return Ok(());
    }

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
pub fn start_protocol_substrate_tx_queue(
    ctx: RelayerContext,
    chain_id: u32,
    store: Arc<Store>,
) -> crate::Result<()> {
    // Start tx_queue only when governance relaying feature is enabled for relayer.
    if !ctx.config.features.governance_relay {
        tracing::warn!(
            "Tx Queue disabled for Protocol-Substrate node ({})",
            chain_id,
        );
        return Ok(());
    }

    let mut shutdown_signal = ctx.shutdown_signal();

    let tx_queue = SubstrateTxQueue::new(ctx, chain_id, store);

    tracing::debug!(
        "Transaction Queue for Protocol-Substrate node({}) Started.",
        chain_id
    );
    let task = async move {
        tokio::select! {
            _ = tx_queue.run::<SubstrateConfig>() => {
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
pub fn start_dkg_substrate_tx_queue(
    ctx: RelayerContext,
    chain_id: u32,
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
            _ = tx_queue.run::<PolkadotConfig>() => {
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

/// Proposal signing backend config
#[allow(clippy::large_enum_variant)]
pub enum ProposalSigningBackendSelector {
    /// None
    None,
    /// Mocked
    Mocked(MockedProposalSigningBackend<SledStore>),
    /// Dkg
    Dkg(DkgProposalSigningBackend),
}
/// utility to configure proposal signing backend
pub async fn make_proposal_signing_backend(
    ctx: &RelayerContext,
    store: Arc<Store>,
    chain_id: u32,
    linked_anchors: Option<Vec<LinkedAnchorConfig>>,
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
            let typed_chain_id = webb_proposals::TypedChainId::Evm(chain_id);
            let dkg_client =
                ctx.substrate_provider::<PolkadotConfig>(&c.node).await?;
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
            let mut signature_bridges: HashSet<webb_proposals::ResourceId> =
                HashSet::new();
            linked_anchors.iter().for_each(|anchor| {
                // using chain_id to ensure that we have only one signature bridge
                let resource_id = match anchor {
                    LinkedAnchorConfig::Raw(target) => {
                        let bytes: [u8; 32] = target.resource_id.into();
                        webb_proposals::ResourceId::from(bytes)
                    }
                    _ => unreachable!("unsupported"),
                };
                signature_bridges.insert(resource_id);
            });
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

/// utility to configure proposal signing backend for substrate chain
pub async fn make_substrate_proposal_signing_backend(
    ctx: &RelayerContext,
    store: Arc<Store>,
    chain_id: u32,
    linked_anchors: Option<Vec<LinkedAnchorConfig>>,
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
                webb_proposals::TypedChainId::Substrate(chain_id);
            let dkg_client =
                ctx.substrate_provider::<PolkadotConfig>(&c.node).await?;
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
            let mut signature_bridges: HashSet<webb_proposals::ResourceId> =
                HashSet::new();
            linked_anchors.iter().for_each(|anchor| {
                // using chain_id to ensure that we have only one signature bridge
                let resource_id = match anchor {
                    LinkedAnchorConfig::Raw(target) => {
                        let bytes: [u8; 32] = target.resource_id.into();
                        webb_proposals::ResourceId::from(bytes)
                    }
                    _ => unreachable!("unsupported"),
                };
                signature_bridges.insert(resource_id);
            });

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
