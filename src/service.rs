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
use webb::substrate::{
    protocol_substrate_runtime::api::RuntimeApi as WebbProtocolRuntimeApi,
    subxt::{self, PairSigner},
};

use crate::config::*;
use crate::context::RelayerContext;
use crate::events_watcher::proposal_signing_backend::*;
use crate::events_watcher::*;
use crate::tx_queue::TxQueue;
/// Type alias for providers
type Client = providers::Provider<providers::Http>;
/// Type alias for the DKG DefaultConfig
type DkgClient = subxt::Client<subxt::DefaultConfig>;
/// Type alias for the DKG RuntimeApi
type DkgRuntime = DkgRuntimeApi<
    subxt::DefaultConfig,
    subxt::DefaultExtra<subxt::DefaultConfig>,
>;
/// Type alias for the WebbProtocol DefaultConfig
type WebbProtocolClient = subxt::Client<subxt::DefaultConfig>;
/// Type alias for the WebbProtocol RuntimeApi
type WebbProtocolRuntime = WebbProtocolRuntimeApi<
    subxt::DefaultConfig,
    subxt::DefaultExtra<subxt::DefaultConfig>,
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
        let provider = ctx.evm_provider(chain_name).await?;
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
                        Pallet::DKGProposals(_) => {
                            // TODO(@shekohex): start the dkg proposals service
                        }
                        Pallet::AnchorBn254(_) => {
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
                            start_substrate_leaves_watcher(
                                ctx,
                                config,
                                client.clone(),
                                node_name.clone(),
                                chain_id,
                                store.clone(),
                            )?;
                        }
                        Pallet::DKGProposals(_) => {
                            unreachable!()
                        }
                        Pallet::DKGProposalHandler(_) => {
                            unreachable!()
                        }
                    }
                }
            }
        };
    }
    Ok(())
}
/// Starts the event watcher for Substrate anchor leaves.
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
fn start_substrate_leaves_watcher(
    ctx: &RelayerContext,
    config: &AnchorBn254PalletConfig,
    client: WebbProtocolClient,
    node_name: String,
    chain_id: U256,
    store: Arc<Store>,
) -> anyhow::Result<()> {
    if !config.events_watcher.enabled {
        tracing::warn!(
            "Substrate Anchor events watcher is disabled for ({}).",
            node_name,
        );
        return Ok(());
    }
    tracing::debug!(
        "Substrate Anchor events watcher for ({}) Started.",
        node_name,
    );
    let node_name2 = node_name.clone();
    let mut shutdown_signal = ctx.shutdown_signal();
    let task = async move {
        let leaves_watcher =
            SubstrateLeavesWatcher::new(node_name.clone(), chain_id);
        let watcher = leaves_watcher.run(node_name, chain_id, client, store);
        tokio::select! {
            _ = watcher => {
                tracing::warn!(
                    "Substrate leaves watcher stopped for ({})",
                    node_name2,
                );
            },
            _ = shutdown_signal.recv() => {
                tracing::trace!(
                    "Stopping substrate leaves watcher for ({})",
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

/// Starts the event watcher for Anchor events.
///
/// Returns Ok(()) if successful, or an error if not.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `config` - Anchor contract configuration
/// * `client` - DKG client
/// * `store` -[Sled](https://sled.rs)-based database store
async fn start_anchor_events_watcher(
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
    let my_ctx = ctx.clone();
    let my_config = config.clone();
    let proposal_signing_backend = config.proposal_signing_backend.clone();
    let task = async move {
        tracing::debug!(
            "Anchor events watcher for ({}) Started.",
            contract_address,
        );
        let leaves_watcher = AnchorLeavesWatcher::default();
        let anchor_leaves_watcher =
            leaves_watcher.run(client.clone(), store.clone(), wrapper.clone());
        // we need to check/match on the proposal signing backend configured for this anchor.
        match proposal_signing_backend {
            ProposalSigningBackendConfig::DkgNode(c) => {
                // if it is the dkg backend, we will need to connect to that node first,
                // and then use the DkgProposalSigningBackend to sign the proposal.
                let dkg_client = my_ctx
                    .substrate_provider::<subxt::DefaultConfig>(&c.node)
                    .await?;
                let pair = my_ctx.substrate_wallet(&c.node).await?;
                let backend = DkgProposalSigningBackend::new(
                    dkg_client,
                    PairSigner::new(pair),
                );
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
            ProposalSigningBackendConfig::Mocked(_) => {
                // if it is the mocked backend, we will use the MockedProposalSigningBackend to sign the proposal.
                // which is a bit simpler than the DkgProposalSigningBackend.
                // get only the linked chains to that anchor.
                let linked_chains =
                    my_config.linked_anchors.iter().flat_map(|c| {
                        my_ctx
                            .config
                            .evm
                            .get(&c.chain)
                            .map(|chain_config| (chain_config, c.address))
                    });
                // then will have to go through our configruation to retrieve the correct
                // signature bridges that are configrued on the linked chains.
                // Note: this assumes that every network will only have one signature bridge configured for it.
                let signature_bridges = linked_chains
                    .flat_map(|(chain_config, address)| {
                        // find the first signature bridge configured on that chain.
                        let maybe_signature_bridge =
                            chain_config.contracts.iter().find(|contract| {
                                matches!(contract, Contract::SignatureBridge(_))
                            });
                        // also find the first the other anchor contract configured on that chain.
                        // but this time with its address.
                        let maybe_anchor_contract = chain_config
                            .contracts
                            .iter()
                            .find(|contract| match contract {
                                Contract::Anchor(c) => {
                                    c.common.address == address
                                }
                                _ => false,
                            });
                        // if we found both, we will return the signature bridge and the anchor contract a long with chain config.
                        maybe_signature_bridge.and_then(|bridge| {
                            maybe_anchor_contract.map(|anchor| {
                                (bridge, anchor.clone(), chain_config)
                            })
                        })
                    })
                    .map(|(bridge_contract, anchor_contract, chain_config)| {
                        let bridge_config = match bridge_contract {
                            Contract::SignatureBridge(c) => c,
                            _ => unreachable!(),
                        };
                        let anchor_config = match anchor_contract {
                            Contract::Anchor(c) => c,
                            _ => unreachable!(),
                        };
                        (bridge_config, anchor_config, chain_config)
                    })
                    .flat_map(|(bridge_config, anchor_config, chain_config)| {
                        // first things first we need the private key of the govenor of the signature bridge.
                        // which is in the anchor config.
                        let private_key =
                            match anchor_config.proposal_signing_backend {
                                ProposalSigningBackendConfig::Mocked(v) => {
                                    v.private_key
                                }
                                _ => return None,
                            };
                        // then we just create the signature bridge metadata.
                        let chain_id = webb_proposals::TypedChainId::Evm(
                            chain_config.chain_id as u32,
                        );
                        let metadata = SignatureBridgeMetadata {
                            address: bridge_config.common.address,
                            chain_id,
                            private_key,
                        };
                        Some((chain_id, metadata))
                    })
                    .collect::<HashMap<_, _>>();
                let backend = MockedProposalSigningBackend::builder()
                    .store(store.clone())
                    .signature_bridges(signature_bridges)
                    .build();
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
