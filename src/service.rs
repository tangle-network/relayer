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
use crate::events_watcher::evm::*;
use crate::events_watcher::proposal_signing_backend::*;
use crate::events_watcher::substrate::*;
use crate::events_watcher::*;
use crate::store::sled::SledStore;
use crate::tx_queue::TxQueue;

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
    subxt::PolkadotExtrinsicParams<subxt::DefaultConfig>,
>;
/// Type alias for [Sled](https://sled.rs)-based database store
type Store = crate::store::sled::SledStore;
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
) -> anyhow::Result<()> {
    // now we go through each chain, in our configuration
    for (chain_name, chain_config) in &ctx.config.evm {
        if !chain_config.enabled {
            continue;
        }
        let chain_id = U256::from(chain_config.chain_id);
        let provider = ctx.evm_provider(chain_name).await?;
        let client = Arc::new(provider);
        tracing::debug!(
            "Starting Background Services for ({}) chain.",
            chain_name
        );

        for contract in &chain_config.contracts {
            match contract {
                Contract::Anchor(config) => {
                    start_evm_anchor_events_watcher(
                        ctx,
                        config,
                        chain_id,
                        client.clone(),
                        store.clone(),
                    )
                    .await?;
                }
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
                    TypedChainId::Evm(id)
                    | TypedChainId::Substrate(id)
                    | TypedChainId::PolkadotParachain(id)
                    | TypedChainId::KusamaParachain(id)
                    | TypedChainId::RococoParachain(id)
                    | TypedChainId::Cosmos(id)
                    | TypedChainId::Solana(id) => id,
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
                        Pallet::AnchorBn254(_) => {
                            unreachable!()
                        }
                        Pallet::SignatureBridge(_) => {
                            unreachable!()
                        }
                        Pallet::VAnchorBn254(_) => {
                            unreachable!()
                        }
                    }
                }
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
                        Pallet::AnchorBn254(config) => {
                            start_substrate_anchor_event_watcher(
                                ctx,
                                config,
                                client.clone(),
                                node_name.clone(),
                                chain_id,
                                store.clone(),
                            )?;
                        }
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
            }
        };
    }
    Ok(())
}
/// Starts the event watcher for Substrate anchor events.
///
/// Returns Ok(()) if successful, or an error if not.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `config` - AnchorBn254 configuration
/// * `client` - WebbProtocol client
/// * `node_name` - Name of the node
/// * `chain_id` - An U256 representing the chain id of the chain
/// * `store` -[Sled](https://sled.rs)-based database store
fn start_substrate_anchor_event_watcher(
    ctx: &RelayerContext,
    config: &AnchorBn254PalletConfig,
    client: WebbProtocolClient,
    node_name: String,
    chain_id: U256,
    store: Arc<Store>,
) -> anyhow::Result<()> {
    if !config.events_watcher.enabled {
        tracing::warn!(
            "Substrate anchor events watcher is disabled for ({}).",
            node_name,
        );
        return Ok(());
    }
    tracing::debug!(
        "Substrate anchor events watcher for ({}) Started.",
        node_name,
    );
    let my_ctx = ctx.clone();
    let my_config = config.clone();
    let mut shutdown_signal = ctx.shutdown_signal();
    let task = async move {
        let watcher = SubstrateAnchorLeavesWatcher::default();
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
            &my_config.linked_anchors[..],
            my_config.proposal_signing_backend,
        )
        .await
        .unwrap();
        match proposal_signing_backend {
            ProposalSigningBackendSelector::Dkg(backend) => {
                let watcher = SubstrateAnchorWatcher::new(
                    backend,
                    &my_config.linked_anchors,
                );
                let substrate_anchor_watcher_task = watcher.run(
                    node_name.clone(),
                    chain_id,
                    client.clone(),
                    store.clone(),
                );
                tokio::select! {
                    _ = substrate_anchor_watcher_task => {
                        tracing::warn!(
                            "Substrate Anchor watcher (DKG Backend) task stopped for ({})",
                            node_name,
                        );
                    },
                    _ = substrate_leaves_watcher_task => {
                        tracing::warn!(
                            "Substrate Anchor leaves watcher stopped for ({})",
                            node_name,
                        );
                    },
                    _ = shutdown_signal.recv() => {
                        tracing::trace!(
                            "Stopping Substrate Anchor watcher (DKG Backend) for ({})",
                            node_name,
                        );
                    },
                }
            }
            ProposalSigningBackendSelector::Mocked(backend) => {
                let watcher = SubstrateAnchorWatcher::new(
                    backend,
                    &my_config.linked_anchors[..],
                );
                let substrate_anchor_watcher_task = watcher.run(
                    node_name.clone(),
                    chain_id,
                    client.clone(),
                    store.clone(),
                );
                tokio::select! {
                    _ = substrate_anchor_watcher_task => {
                        tracing::warn!(
                            "Substrate Anchor watcher (Mocked Backend) task stopped for ({})",
                            node_name,
                        );
                    },
                    _ = substrate_leaves_watcher_task => {
                        tracing::warn!(
                            "Substrate Anchor leaves watcher stopped for ({})",
                            node_name,
                        );
                    },
                    _ = shutdown_signal.recv() => {
                        tracing::trace!(
                            "Stopping Substrate Anchor watcher (Mocked Backend) for ({})",
                            node_name,
                        );
                    },
                }
            }
            ProposalSigningBackendSelector::None => {
                tracing::debug!(
                    "No backend configured for proposal signing..!"
                );
            }
        }
    };
    // kick off the watcher.
    tokio::task::spawn(task);
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
) -> anyhow::Result<()> {
    if !config.events_watcher.enabled {
        tracing::warn!(
            "Substrate vanchor events watcher is disabled for ({}).",
            node_name,
        );
        return Ok(());
    }
    tracing::debug!(
        "Substrate vanchor events watcher for ({}) Started.",
        node_name,
    );
    let node_name2 = node_name.clone();
    let mut shutdown_signal = ctx.shutdown_signal();
    let task = async move {
        let leaves_watcher = SubstrateVAnchorLeavesWatcher::default();
        let watcher = leaves_watcher.run(node_name, chain_id, client, store);
        tokio::select! {
            _ = watcher => {
                tracing::warn!(
                    "Substrate vanchor leaves watcher stopped for ({})",
                    node_name2,
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping substrate vanchor leaves watcher for ({})",
                    node_name2,
                );
            },
        }
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
) -> anyhow::Result<()> {
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
) -> anyhow::Result<()> {
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
        let leaves_watcher = VAnchorLeavesWatcher::default();
        let vanchor_leaves_watcher =
            leaves_watcher.run(client.clone(), store.clone(), wrapper.clone());
        let proposal_signing_backend = make_proposal_signing_backend(
            &my_ctx,
            store.clone(),
            chain_id,
            &my_config.linked_anchors,
            my_config.proposal_signing_backend,
        )
        .await?;
        match proposal_signing_backend {
            ProposalSigningBackendSelector::Dkg(backend) => {
                let watcher = VAnchorWatcher::new(backend);
                let vanchor_watcher_task = watcher.run(client, store, wrapper);
                tokio::select! {
                    _ = vanchor_watcher_task => {
                        tracing::warn!(
                            "VAnchor watcher task stopped for ({})",
                            contract_address,
                        );
                    },
                    _ = vanchor_leaves_watcher => {
                        tracing::warn!(
                            "VAnchor leaves watcher stopped for ({})",
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
                let watcher = VAnchorWatcher::new(backend);
                let vanchor_watcher_task = watcher.run(client, store, wrapper);
                tokio::select! {
                    _ = vanchor_watcher_task => {
                        tracing::warn!(
                            "VAnchor watcher task stopped for ({})",
                            contract_address,
                        );
                    },
                    _ = vanchor_leaves_watcher => {
                        tracing::warn!(
                            "VAnchor leaves watcher stopped for ({})",
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
                tracing::debug!(
                    "No backend configured for proposal signing..!"
                );
            }
        };

        Result::<_, anyhow::Error>::Ok(())
    };
    // kick off the watcher.
    tokio::task::spawn(task);
    Ok(())
}

/// Starts the event watcher for EVM Anchor events.
///
/// Returns Ok(()) if successful, or an error if not.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `config` - Anchor contract configuration
/// * `client` - EVM Chain api client
/// * `store` -[Sled](https://sled.rs)-based database store
async fn start_evm_anchor_events_watcher(
    ctx: &RelayerContext,
    config: &AnchorContractConfig,
    chain_id: U256,
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
    let my_ctx = ctx.clone();
    let my_config = config.clone();
    let task = async move {
        tracing::debug!(
            "Anchor events watcher for ({}) Started.",
            contract_address,
        );
        let leaves_watcher = AnchorLeavesWatcher::default();
        let anchor_leaves_watcher =
            leaves_watcher.run(client.clone(), store.clone(), wrapper.clone());
        let proposal_signing_backend = make_proposal_signing_backend(
            &my_ctx,
            store.clone(),
            chain_id,
            &my_config.linked_anchors,
            my_config.proposal_signing_backend,
        )
        .await?;
        match proposal_signing_backend {
            ProposalSigningBackendSelector::Dkg(backend) => {
                let watcher = AnchorWatcher::new(backend);
                let anchor_watcher_task = watcher.run(client, store, wrapper);
                tokio::select! {
                    _ = anchor_watcher_task => {
                        tracing::warn!(
                            "Anchor watcher task stopped for ({})",
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
            }
            ProposalSigningBackendSelector::Mocked(backend) => {
                let watcher = AnchorWatcher::new(backend);
                let anchor_watcher_task = watcher.run(client, store, wrapper);
                tokio::select! {
                    _ = anchor_watcher_task => {
                        tracing::warn!(
                            "Anchor watcher task stopped for ({})",
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
            }
            ProposalSigningBackendSelector::None => {
                tracing::debug!(
                    "No backend configured for proposal signing..!"
                );
            }
        };

        Result::<_, anyhow::Error>::Ok(())
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

/// Starts the event watcher for Signature Bridge Pallet.
async fn start_substrate_signature_bridge_events_watcher(
    ctx: RelayerContext,
    config: &SignatureBridgePalletConfig,
    client: WebbProtocolClient,
    node_name: String,
    chain_id: U256,
    store: Arc<Store>,
) -> anyhow::Result<()> {
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
            node_name.clone(),
            chain_id,
            client.clone(),
            ctx.clone(),
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
    linked_anchors: &[LinkedAnchorConfig],
    proposal_signing_backend: Option<ProposalSigningBackendConfig>,
) -> anyhow::Result<ProposalSigningBackendSelector> {
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
                .flat_map(|c| ctx.config.evm.get(&c.chain));
            // then will have to go through our configruation to retrieve the correct
            // signature bridges that are configrued on the linked chains.
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
        None => Ok(ProposalSigningBackendSelector::None),
    }
}

async fn make_substrate_proposal_signing_backend(
    ctx: &RelayerContext,
    store: Arc<Store>,
    chain_id: U256,
    linked_anchors: &[SubstrateLinkedAnchorConfig],
    proposal_signing_backend: Option<ProposalSigningBackendConfig>,
) -> anyhow::Result<ProposalSigningBackendSelector> {
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
        None => Ok(ProposalSigningBackendSelector::None),
    }
}
