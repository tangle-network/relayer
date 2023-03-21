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
use ethereum_types::U256;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_http::cors::Any;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use webb::evm::ethers::providers;
use webb_bridge_registry_backends::dkg::DkgBridgeRegistryBackend;
use webb_ew_dkg::DKGMetadataWatcher;
use webb_ew_dkg::DKGProposalHandlerWatcher;
use webb_ew_dkg::DKGPublicKeyChangedHandler;
use webb_ew_dkg::ProposalSignedHandler;
use webb_ew_substrate::MaintainerSetEventHandler;
use webb_ew_substrate::SubstrateBridgeEventWatcher;
use webb_ew_substrate::SubstrateVAnchorEventWatcher;

use webb::substrate::subxt::config::{PolkadotConfig, SubstrateConfig};
use webb::substrate::subxt::{tx::PairSigner, OnlineClient};
use webb_bridge_registry_backends::mocked::MockedBridgeRegistryBackend;
use webb_event_watcher_traits::evm::{BridgeWatcher, EventWatcher};
use webb_event_watcher_traits::substrate::SubstrateBridgeWatcher;
use webb_event_watcher_traits::SubstrateEventWatcher;
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
    SubstrateVAnchorDepositHandler, SubstrateVAnchorEncryptedOutputHandler,
    SubstrateVAnchorLeavesHandler,
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
use webb_relayer_handlers::{
    handle_fee_info, handle_socket_info, websocket_handler,
};

use webb_relayer_handlers::routes::info::handle_relayer_info;
use webb_relayer_handlers::routes::leaves::{
    handle_leaves_cache_evm, handle_leaves_cache_substrate,
};
use webb_relayer_handlers::routes::{encrypted_outputs, metric};
use webb_relayer_store::SledStore;
use webb_relayer_tx_queue::{evm::TxQueue, substrate::SubstrateTxQueue};

/// Type alias for providers
pub type Client = providers::Provider<providers::Http>;
/// Type alias for the DKG DefaultConfig
pub type DkgClient = OnlineClient<PolkadotConfig>;
/// Type alias for the WebbProtocol DefaultConfig
pub type WebbProtocolClient = OnlineClient<SubstrateConfig>;
/// Type alias for [Sled](https://sled.rs)-based database store
pub type Store = SledStore;

/// Sets up the web socket server for the relayer,  routing (endpoint queries / requests mapped to
/// handled code) and instantiates the database store. Allows clients to interact with the relayer.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration and database
pub async fn build_web_services(ctx: RelayerContext) -> crate::Result<()> {
    let socket_addr = SocketAddr::new([0, 0, 0, 0].into(), ctx.config.port);
    let api = Router::new()
        .route("/ip", get(handle_socket_info))
        .route("/info", get(handle_relayer_info))
        .route(
            "/leaves/evm/:chain_id/:contract",
            get(handle_leaves_cache_evm),
        )
        .route(
            "/leaves/substrate/:chain_id/:tree_id/:pallet_id",
            get(handle_leaves_cache_substrate),
        )
        .route(
            "/encrypted_outputs/evm/:chain_id/:contract_address",
            get(encrypted_outputs::handle_encrypted_outputs_cache_evm),
        )
        .route("/metrics", get(metric::handle_metric_info))
        .route(
            "/metrics/evm/:chain_id/:contract",
            get(metric::handle_evm_metric_info),
        )
        .route(
            "/metrics/substrate/:chain_id/:tree_id/:pallet_id",
            get(metric::handle_substrate_metric_info),
        )
        .route(
            "/fee_info/:chain_id/:vanchor/:gas_amount",
            get(handle_fee_info),
        );

    let app = Router::new()
        .nest("/api/v1", api)
        .route("/ws", get(websocket_handler))
        .layer(CorsLayer::new().allow_origin(Any))
        .layer(TraceLayer::new_for_http())
        .with_state(Arc::new(ctx))
        .into_make_service_with_connect_info::<SocketAddr>();

    tracing::info!("Starting the server on {}", socket_addr);
    axum::Server::bind(&socket_addr).serve(app).await?;
    Ok(())
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
                                chain_id,
                                store.clone(),
                            )?;
                        }
                        Pallet::Dkg(config) => {
                            start_dkg_pallet_watcher(
                                ctx,
                                config,
                                client.clone(),
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
                                chain_id,
                                store.clone(),
                            )?;
                        }
                        Pallet::SignatureBridge(config) => {
                            start_substrate_signature_bridge_events_watcher(
                                ctx.clone(),
                                config,
                                client.clone(),
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
/// * `chain_id` - An u32 representing the chain id of the chain
/// * `store` -[Sled](https://sled.rs)-based database store
pub fn start_substrate_vanchor_event_watcher(
    ctx: &RelayerContext,
    config: &VAnchorBn254PalletConfig,
    client: WebbProtocolClient,
    chain_id: u32,
    store: Arc<Store>,
) -> crate::Result<()> {
    if !config.events_watcher.enabled {
        tracing::warn!(
            "Substrate VAnchor events watcher is disabled for ({}).",
            chain_id,
        );
        return Ok(());
    }
    tracing::debug!(
        "Substrate VAnchor events watcher for ({}) Started.",
        chain_id,
    );

    let my_ctx = ctx.clone();
    let my_config = config.clone();
    let mut shutdown_signal = ctx.shutdown_signal();
    let metrics = ctx.metrics.clone();
    let task = async move {
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
                let deposit_handler = SubstrateVAnchorDepositHandler::new(
                    backend,
                    my_config.linked_anchors.unwrap(),
                );
                let leaves_handler = SubstrateVAnchorLeavesHandler::default();
                let encrypted_output_handler =
                    SubstrateVAnchorEncryptedOutputHandler::default();

                let watcher = SubstrateVAnchorEventWatcher::default();
                let substrate_vanchor_watcher_task = watcher.run(
                    chain_id,
                    client.clone().into(),
                    store.clone(),
                    my_config.events_watcher,
                    vec![
                        Box::new(deposit_handler),
                        Box::new(leaves_handler),
                        Box::new(encrypted_output_handler),
                    ],
                    metrics.clone(),
                );

                tokio::select! {
                    _ = substrate_vanchor_watcher_task => {
                        tracing::warn!(
                            "Substrate VAnchor watcher (DKG Backend) task stopped for ({})",
                            chain_id,
                        );
                    },

                    _ = shutdown_signal.recv() => {
                        tracing::trace!(
                            "Stopping Substrate VAnchor watcher (DKG Backend) for ({})",
                            chain_id,
                        );
                    },
                }
            }
            ProposalSigningBackendSelector::Mocked(backend) => {
                // its safe to use unwrap on linked_anchors here
                // since this option is always going to return Some(value).
                let deposit_handler = SubstrateVAnchorDepositHandler::new(
                    backend,
                    my_config.linked_anchors.unwrap(),
                );
                let leaves_handler = SubstrateVAnchorLeavesHandler::default();
                let encrypted_output_handler =
                    SubstrateVAnchorEncryptedOutputHandler::default();

                let watcher = SubstrateVAnchorEventWatcher::default();
                let substrate_vanchor_watcher_task = watcher.run(
                    chain_id,
                    client.clone().into(),
                    store.clone(),
                    my_config.events_watcher,
                    vec![
                        Box::new(deposit_handler),
                        Box::new(leaves_handler),
                        Box::new(encrypted_output_handler),
                    ],
                    metrics.clone(),
                );
                tokio::select! {
                    _ = substrate_vanchor_watcher_task => {
                        tracing::warn!(
                            "Substrate VAnchor watcher (Mocked Backend) task stopped for ({})",
                            chain_id,
                        );
                    },
                    _ = shutdown_signal.recv() => {
                        tracing::trace!(
                            "Stopping Substrate VAnchor watcher (Mocked Backend) for ({})",
                            chain_id,
                        );
                    },
                }
            }
            ProposalSigningBackendSelector::None => {
                let leaves_handler = SubstrateVAnchorLeavesHandler::default();
                let encrypted_output_handler =
                    SubstrateVAnchorEncryptedOutputHandler::default();

                let watcher = SubstrateVAnchorEventWatcher::default();
                let substrate_vanchor_watcher_task = watcher.run(
                    chain_id,
                    client.clone().into(),
                    store.clone(),
                    my_config.events_watcher,
                    vec![
                        Box::new(leaves_handler),
                        Box::new(encrypted_output_handler),
                    ],
                    metrics.clone(),
                );
                tokio::select! {
                    _ = substrate_vanchor_watcher_task => {
                        tracing::warn!(
                            "Substrate VAnchor watcher task stopped for ({})",
                            chain_id,
                        );
                    },
                    _ = shutdown_signal.recv() => {
                        tracing::trace!(
                            "Stopping Substrate VAnchor watcher task for ({})",
                            chain_id,
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
/// * `chain_id` - An u32 representing the chain id of the chain
/// * `store` -[Sled](https://sled.rs)-based database store
pub fn start_dkg_proposal_handler(
    ctx: &RelayerContext,
    config: &DKGProposalHandlerPalletConfig,
    client: DkgClient,
    chain_id: u32,
    store: Arc<Store>,
) -> crate::Result<()> {
    // check first if we should start the events watcher for this contract.
    if !config.events_watcher.enabled {
        tracing::warn!(
            "DKG Proposal Handler events watcher is disabled for ({}).",
            chain_id,
        );
        return Ok(());
    }
    tracing::debug!(
        "DKG Proposal Handler events watcher for ({}) Started.",
        chain_id,
    );
    let mut shutdown_signal = ctx.shutdown_signal();
    let metrics = ctx.metrics.clone();
    let my_config = config.clone();
    let task = async move {
        let proposal_handler_watcher = DKGProposalHandlerWatcher::default();
        let proposal_signed_handler = ProposalSignedHandler::default();
        let proposal_handler_watcher_task = proposal_handler_watcher.run(
            chain_id,
            client.into(),
            store,
            my_config.events_watcher,
            vec![Box::new(proposal_signed_handler)],
            metrics,
        );
        tokio::select! {
            _ = proposal_handler_watcher_task => {
                tracing::warn!(
                    "DKG Proposal Handler events watcher stopped for ({})",
                    chain_id,
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping DKG Proposal Handler events watcher for ({})",
                    chain_id,
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
/// * `chain_id` - An u32 representing the chain id of the chain
/// * `store` -[Sled](https://sled.rs)-based database store
pub fn start_dkg_pallet_watcher(
    ctx: &RelayerContext,
    config: &DKGPalletConfig,
    client: DkgClient,
    chain_id: u32,
    store: Arc<Store>,
) -> crate::Result<()> {
    // check first if we should start the events watcher for this pallet.
    if !config.events_watcher.enabled {
        tracing::warn!(
            "DKG Pallet events watcher is disabled for ({}).",
            chain_id,
        );
        return Ok(());
    }
    tracing::debug!("DKG Pallet events watcher for ({}) Started.", chain_id,);
    let mut shutdown_signal = ctx.shutdown_signal();
    let webb_config = ctx.config.clone();
    let metrics = ctx.metrics.clone();
    let my_config = config.clone();
    let task = async move {
        let dkg_event_watcher = DKGMetadataWatcher::default();
        let public_key_changed_handler =
            DKGPublicKeyChangedHandler::new(webb_config);

        let dkg_event_watcher_task = dkg_event_watcher.run(
            chain_id,
            client.into(),
            store,
            my_config.events_watcher,
            vec![Box::new(public_key_changed_handler)],
            metrics,
        );
        tokio::select! {
            _ = dkg_event_watcher_task => {
                tracing::warn!(
                    "DKG Pallet events watcher stopped for ({})",
                    chain_id,
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping DKG Pallet events watcher for ({})",
                    chain_id,
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
    let default_leaf: U256 = wrapper.contract.get_zero_hash(0).call().await?;

    let mut default_leaf_bytes = [0u8; 32];
    default_leaf.to_big_endian(&mut default_leaf_bytes);
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
                let bridge_registry = DkgBridgeRegistryBackend::new(
                    OnlineClient::<PolkadotConfig>::new().await?,
                );
                let deposit_handler =
                    VAnchorDepositHandler::new(backend, bridge_registry);
                let leaves_handler = VAnchorLeavesHandler::new(
                    store.clone(),
                    default_leaf_bytes.to_vec(),
                );
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
                let bridge_registry =
                    MockedBridgeRegistryBackend::builder().build();
                let deposit_handler =
                    VAnchorDepositHandler::new(backend, bridge_registry);
                let leaves_handler = VAnchorLeavesHandler::new(
                    store.clone(),
                    default_leaf_bytes.to_vec(),
                );
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
                let leaves_handler = VAnchorLeavesHandler::new(
                    store.clone(),
                    default_leaf_bytes.to_vec(),
                );
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
                let bridge_registry = DkgBridgeRegistryBackend::new(
                    OnlineClient::<PolkadotConfig>::new().await?,
                );
                let deposit_handler =
                    OpenVAnchorDepositHandler::new(backend, bridge_registry);
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
                let bridge_registry =
                    MockedBridgeRegistryBackend::builder().build();
                let deposit_handler =
                    OpenVAnchorDepositHandler::new(backend, bridge_registry);
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
    chain_id: u32,
    store: Arc<Store>,
) -> crate::Result<()> {
    if !config.events_watcher.enabled {
        tracing::warn!(
            "Substrate Signature Bridge events watcher is disabled for ({}).",
            chain_id,
        );
        return Ok(());
    }
    let mut shutdown_signal = ctx.shutdown_signal();
    let my_config = config.clone();
    let task = async move {
        tracing::debug!(
            "Substrate Signature Bridge watcher for ({}) Started.",
            chain_id
        );
        let substrate_bridge_watcher = SubstrateBridgeEventWatcher::default();
        let bridge_event_handler = MaintainerSetEventHandler::default();
        let events_watcher_task = SubstrateEventWatcher::run(
            &substrate_bridge_watcher,
            chain_id,
            client.clone().into(),
            store.clone(),
            my_config.events_watcher,
            vec![Box::new(bridge_event_handler)],
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
                    chain_id
                );
            },
            _ = cmd_handler_task => {
                tracing::warn!(
                    "Substrate signature bridge cmd handler task stopped for ({})",
                    chain_id
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping Substrate Signature Bridge watcher for ({})",
                    chain_id,
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

            // Check if linked anchors are provided.
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
