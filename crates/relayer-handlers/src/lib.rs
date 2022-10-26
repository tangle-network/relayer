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

//! Relayer handlers for HTTP/Socket calls

#![allow(clippy::large_enum_variant)]
#![warn(missing_docs)]
use std::collections::HashMap;
use std::convert::Infallible;
use std::error::Error;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use ethereum_types::Address;
use futures::prelude::*;
use serde::Serialize;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use warp::ws::Message;
use webb::evm::ethers::{
    core::k256::SecretKey,
    signers::{LocalWallet, Signer},
};
use webb::substrate::subxt::ext::sp_core::Pair;
use webb_proposals::{
    ResourceId, SubstrateTargetSystem, TargetSystem, TypedChainId,
};
use webb_relayer_context::RelayerContext;
use webb_relayer_handler_utils::{
    Command, CommandResponse, CommandStream, CommandType, EvmCommand,
    IpInformationResponse, SubstrateCommand,
};
use webb_relayer_store::{EncryptedOutputCacheStore, LeafCacheStore};
use webb_relayer_tx_relay::evm::vanchor::handle_vanchor_relay_tx;
use webb_relayer_tx_relay::substrate::mixer::handle_substrate_mixer_relay_tx;
use webb_relayer_tx_relay::substrate::vanchor::handle_substrate_vanchor_relay_tx;
use webb_relayer_utils::metric::Metrics;

/// Sets up a websocket connection.
///
/// Returns `Ok(())` on success
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `stream` - Websocket stream
pub async fn accept_connection(
    ctx: &RelayerContext,
    stream: warp::ws::WebSocket,
) -> webb_relayer_utils::Result<()> {
    let (mut tx, mut rx) = stream.split();

    // Wait for client to send over text (such as relay transaction requests)
    while let Some(msg) = rx.try_next().await? {
        if let Ok(text) = msg.to_str() {
            handle_text(ctx, text, &mut tx).await?;
        }
    }
    Ok(())
}
/// Sets up a websocket channels for message sending.
///
/// This is primarily used for transaction relaying. The intention is
/// that a user will send formatted relay requests to the relayer using
/// the websocket. The command will be extracted and sent to `handle_cmd`
/// if successfully deserialized.
///
/// Returns `Ok(())` on success
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `v` - The text (usually in a JSON form) message to be handled.
/// * `tx` - A mutable Trait implementation of the `warp::ws::Sender` trait
pub async fn handle_text<TX>(
    ctx: &RelayerContext,
    v: &str,
    tx: &mut TX,
) -> webb_relayer_utils::Result<()>
where
    TX: Sink<Message> + Unpin,
    TX::Error: Error + Send + Sync + 'static,
{
    // for every connection, we create a new channel, where we will use to send messages
    // over it.
    let (my_tx, my_rx) = mpsc::channel(50);
    let res_stream = ReceiverStream::new(my_rx);
    match serde_json::from_str(v) {
        Ok(cmd) => {
            handle_cmd(ctx.clone(), cmd, my_tx).await;
            // Send back the response, usually a transaction hash
            // from processing the transaction relaying command.
            res_stream
                .fuse()
                .map(|v| serde_json::to_string(&v).expect("bad value"))
                .inspect(|v| tracing::trace!("Sending: {}", v))
                .map(Message::text)
                .map(Result::Ok)
                .forward(tx)
                .map_err(|_| webb_relayer_utils::Error::FailedToSendResponse)
                .await?;
        }
        Err(e) => {
            tracing::warn!("Got invalid payload: {:?}", e);
            let error = CommandResponse::Error(e.to_string());
            let value = serde_json::to_string(&error)?;
            tx.send(Message::text(value))
                .map_err(|_| webb_relayer_utils::Error::FailedToSendResponse)
                .await?;
        }
    };
    Ok(())
}

/// Handles the `ip` address response
///
/// Returns a Result with the `IpInformationResponse` on success
///
/// # Arguments
///
/// * `ip` - Option containing the IP address
pub async fn handle_ip_info(
    ip: Option<IpAddr>,
) -> Result<impl warp::Reply, Infallible> {
    Ok(warp::reply::json(&IpInformationResponse {
        ip: ip.unwrap().to_string(),
    }))
}
/// Handles the socket address response
///
/// Returns a Result with the `IpInformationResponse` on success
///
/// # Arguments
///
/// * `ip` - Option containing the socket address
pub async fn handle_socket_info(
    ip: Option<SocketAddr>,
) -> Result<impl warp::Reply, Infallible> {
    Ok(warp::reply::json(&IpInformationResponse {
        ip: ip.unwrap().ip().to_string(),
    }))
}
/// Handles relayer configuration requests
///
/// Returns a Result with the `RelayerConfigurationResponse` on success
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
pub async fn handle_relayer_info(
    ctx: Arc<RelayerContext>,
) -> Result<impl warp::Reply, Infallible> {
    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct RelayerInformationResponse {
        #[serde(flatten)]
        config: webb_relayer_config::WebbRelayerConfig,
    }
    // clone the original config, to update it with accounts.
    let mut config = ctx.config.clone();

    let _ = config
        .evm
        .values_mut()
        .filter(|v| v.beneficiary.is_none())
        .try_for_each(|v| {
            let key = v
                .private_key
                .as_ref()
                .ok_or(webb_relayer_utils::Error::MissingSecrets)?;
            let key = SecretKey::from_be_bytes(key.as_bytes())?;
            let wallet = LocalWallet::from(key);
            v.beneficiary = Some(wallet.address());
            webb_relayer_utils::Result::Ok(())
        });
    let _ = config
        .substrate
        .values_mut()
        .filter(|v| v.beneficiary.is_none())
        .try_for_each(|v| {
            let suri = v
                .suri
                .as_ref()
                .ok_or(webb_relayer_utils::Error::MissingSecrets)?;
            v.beneficiary = Some(suri.public());
            webb_relayer_utils::Result::Ok(())
        });
    Ok(warp::reply::json(&RelayerInformationResponse { config }))
}
/// Handles leaf data requests for evm
///
/// Returns a Result with the `LeafDataResponse` on success
///
/// # Arguments
///
/// * `store` - [Sled](https://sled.rs)-based database store
/// * `chain_id` - An u32 representing the chain id of the chain to query
/// * `contract` - An address of the contract to query
/// * `ctx` - RelayContext reference that holds the configuration
pub async fn handle_leaves_cache_evm(
    store: Arc<webb_relayer_store::sled::SledStore>,
    chain_id: u32,
    contract: Address,
    ctx: Arc<RelayerContext>,
) -> Result<impl warp::Reply, Infallible> {
    let config = ctx.config.clone();

    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct LeavesCacheResponse {
        leaves: Vec<Vec<u8>>,
        last_queried_block: u64,
    }
    // Unsupported feature response
    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct UnsupportedFeature {
        message: String,
    }

    // check if data query is enabled for relayer
    if !config.features.data_query {
        tracing::warn!("Data query is not enabled for relayer.");
        return Ok(warp::reply::with_status(
            warp::reply::json(&UnsupportedFeature {
                message: "Data query is not enabled for relayer.".to_string(),
            }),
            warp::http::StatusCode::FORBIDDEN,
        ));
    }

    // check if chain is supported
    let chain = match ctx.config.evm.get(&chain_id.to_string()) {
        Some(v) => v,
        None => {
            tracing::warn!("Unsupported Chain: {}", chain_id);
            return Ok(warp::reply::with_status(
                warp::reply::json(&UnsupportedFeature {
                    message: format!("Unsupported Chain: {chain_id}"),
                }),
                warp::http::StatusCode::BAD_REQUEST,
            ));
        }
    };

    let supported_contracts: HashMap<_, _> = chain
        .contracts
        .iter()
        .cloned()
        .filter_map(|c| match c {
            webb_relayer_config::evm::Contract::VAnchor(c) => {
                Some((c.common.address, c.events_watcher))
            }
            _ => None,
        })
        .collect();

    // check if contract is supported
    let event_watcher_config = match supported_contracts.get(&contract) {
        Some(config) => config,
        None => {
            tracing::warn!(
                "Unsupported Contract: {:?} for chaind : {}",
                contract,
                chain_id
            );
            return Ok(warp::reply::with_status(
                warp::reply::json(&UnsupportedFeature {
                    message: format!(
                        "Unsupported Contract: {} for chaind : {}",
                        contract, chain_id
                    ),
                }),
                warp::http::StatusCode::BAD_REQUEST,
            ));
        }
    };
    // check if data query is enabled for contract
    if !event_watcher_config.enable_data_query {
        tracing::warn!("Enbable data query for contract : ({})", contract);
        return Ok(warp::reply::with_status(
            warp::reply::json(&UnsupportedFeature {
                message: format!(
                    "Enbable data query for contract : ({})",
                    contract
                ),
            }),
            warp::http::StatusCode::FORBIDDEN,
        ));
    }
    // create history store key
    let src_target_system =
        TargetSystem::new_contract_address(contract.to_fixed_bytes());
    let src_typed_chain_id = TypedChainId::Evm(chain_id);
    let history_store_key =
        ResourceId::new(src_target_system, src_typed_chain_id);
    let leaves = store.get_leaves(history_store_key).unwrap();
    let last_queried_block = store
        .get_last_deposit_block_number(history_store_key)
        .unwrap();

    Ok(warp::reply::with_status(
        warp::reply::json(&LeavesCacheResponse {
            leaves,
            last_queried_block,
        }),
        warp::http::StatusCode::OK,
    ))
}
/// Handles leaf data requests for substrate
///
/// Returns a Result with the `LeafDataResponse` on success
///
/// # Arguments
///
/// * `store` - [Sled](https://sled.rs)-based database store
/// * `chain_id` - An u32 representing the chain id of the chain to query
/// * `tree_id` - Tree id of the the source system to query
/// * `pallet_id` - Pallet id of the the source system to query
/// * `ctx` - RelayContext reference that holds the configuration
pub async fn handle_leaves_cache_substrate(
    store: Arc<webb_relayer_store::sled::SledStore>,
    chain_id: u32,
    tree_id: u32,
    pallet_id: u8,
    ctx: Arc<RelayerContext>,
) -> Result<impl warp::Reply, Infallible> {
    let config = ctx.config.clone();
    // Leaves cache response
    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct LeavesCacheResponse {
        leaves: Vec<Vec<u8>>,
        last_queried_block: u64,
    }
    // Unsupported feature response
    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct UnsupportedFeature {
        message: String,
    }
    // check if data querying is enabled
    if !config.features.data_query {
        tracing::warn!("Data query is not enabled for relayer.");
        return Ok(warp::reply::with_status(
            warp::reply::json(&UnsupportedFeature {
                message: "Data query is not enabled for relayer.".to_string(),
            }),
            warp::http::StatusCode::FORBIDDEN,
        ));
    }

    // create history store key
    let src_typed_chain_id = TypedChainId::Substrate(chain_id);
    let target = SubstrateTargetSystem::builder()
        .pallet_index(pallet_id)
        .tree_id(tree_id)
        .build();
    let src_target_system = TargetSystem::Substrate(target);
    let history_store_key =
        ResourceId::new(src_target_system, src_typed_chain_id);

    let leaves = store.get_leaves(history_store_key).unwrap();
    let last_queried_block = store
        .get_last_deposit_block_number(history_store_key)
        .unwrap();

    Ok(warp::reply::with_status(
        warp::reply::json(&LeavesCacheResponse {
            leaves,
            last_queried_block,
        }),
        warp::http::StatusCode::OK,
    ))
}
/// Handles leaf data requests for Cosmos-SDK chains(cosmwasm)
///
/// Returns a Result with the `LeafDataResponse` on success
///
/// # Arguments
///
/// * `store` - [Sled](https://sled.rs)-based database store
/// * `chain_id` - An U256 representing the chain id of the chain to query
/// * `contract` - An address of the contract to query
/// * `ctx` - RelayContext reference that holds the configuration
pub async fn handle_leaves_cache_cosmwasm(
    store: Arc<webb_relayer_store::sled::SledStore>,
    chain_id: u32,
    contract: String,
    ctx: Arc<RelayerContext>,
) -> Result<impl warp::Reply, Infallible> {
    let config = ctx.config.clone();

    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct LeavesCacheResponse {
        leaves: Vec<Vec<u8>>,
        last_queried_block: u64,
    }
    // Unsupported feature response
    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct UnsupportedFeature {
        message: String,
    }

    // check if data query is enabled for relayer
    if !config.features.data_query {
        tracing::warn!("Data query is not enabled for relayer.");
        return Ok(warp::reply::with_status(
            warp::reply::json(&UnsupportedFeature {
                message: "Data query is not enabled for relayer.".to_string(),
            }),
            warp::http::StatusCode::FORBIDDEN,
        ));
    }

    // check if chain is supported
    let chain = match ctx.config.cosmwasm.get(&chain_id.to_string()) {
        Some(v) => v,
        None => {
            tracing::warn!("Unsupported Chain: {}", chain_id);
            return Ok(warp::reply::with_status(
                warp::reply::json(&UnsupportedFeature {
                    message: format!("Unsupported Chain: {chain_id}"),
                }),
                warp::http::StatusCode::BAD_REQUEST,
            ));
        }
    };

    let supported_contracts: HashMap<_, _> = chain
        .contracts
        .iter()
        .cloned()
        .filter_map(|c| match c {
            webb_relayer_config::cosmwasm::CosmwasmContract::VAnchor(c) => {
                Some((c.common.address, c.events_watcher))
            }
            _ => None,
        })
        .collect();

    // check if contract is supported
    let event_watcher_config = match supported_contracts.get(&contract) {
        Some(config) => config,
        None => {
            tracing::warn!(
                "Unsupported Contract: {:?} for chaind : {}",
                contract,
                chain_id
            );
            return Ok(warp::reply::with_status(
                warp::reply::json(&UnsupportedFeature {
                    message: format!(
                        "Unsupported Contract: {} for chaind : {}",
                        contract, chain_id
                    ),
                }),
                warp::http::StatusCode::BAD_REQUEST,
            ));
        }
    };
    // check if data query is enabled for contract
    if !event_watcher_config.enable_data_query {
        tracing::warn!(
            "Enbable data query for contract : ({})",
            contract.to_string()
        );
        return Ok(warp::reply::with_status(
            warp::reply::json(&UnsupportedFeature {
                message: format!(
                    "Enbable data query for contract : ({})",
                    contract
                ),
            }),
            warp::http::StatusCode::FORBIDDEN,
        ));
    }
    let leaves = store.get_leaves(chain_id).unwrap();
    let last_queried_block =
        store.get_last_deposit_block_number(chain_id).unwrap();

    Ok(warp::reply::with_status(
        warp::reply::json(&LeavesCacheResponse {
            leaves,
            last_queried_block,
        }),
        warp::http::StatusCode::OK,
    ))
}

/// Handles encrypted outputs data requests for evm
///
/// Returns a Result with the `EncryptedOutputDataResponse` on success
///
/// # Arguments
///
/// * `store` - [Sled](https://sled.rs)-based database store
/// * `chain_id` - An U256 representing the chain id of the chain to query
/// * `contract` - An address of the contract to query
/// * `ctx` - RelayContext reference that holds the configuration
pub async fn handle_encrypted_outputs_cache_evm(
    store: Arc<webb_relayer_store::sled::SledStore>,
    chain_id: u32,
    contract: Address,
    ctx: Arc<RelayerContext>,
) -> Result<impl warp::Reply, Infallible> {
    let config = ctx.config.clone();

    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct EncryptedOutputsCacheResponse {
        encrypted_outputs: Vec<Vec<u8>>,
        last_queried_block: u64,
    }
    // Unsupported feature response
    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct UnsupportedFeature {
        message: String,
    }

    // check if data query is enabled for relayer
    if !config.features.data_query {
        tracing::warn!("Data query is not enabled for relayer.");
        return Ok(warp::reply::with_status(
            warp::reply::json(&UnsupportedFeature {
                message: "Data query is not enabled for relayer.".to_string(),
            }),
            warp::http::StatusCode::FORBIDDEN,
        ));
    }

    // check if chain is supported
    let chain = match ctx.config.evm.get(&chain_id.to_string()) {
        Some(v) => v,
        None => {
            tracing::warn!("Unsupported Chain: {}", chain_id);
            return Ok(warp::reply::with_status(
                warp::reply::json(&UnsupportedFeature {
                    message: format!("Unsupported Chain: {chain_id}"),
                }),
                warp::http::StatusCode::BAD_REQUEST,
            ));
        }
    };

    let supported_contracts: HashMap<_, _> = chain
        .contracts
        .iter()
        .cloned()
        .filter_map(|c| match c {
            webb_relayer_config::evm::Contract::VAnchor(c) => {
                Some((c.common.address, c.events_watcher))
            }
            _ => None,
        })
        .collect();

    // check if contract is supported
    let event_watcher_config = match supported_contracts.get(&contract) {
        Some(config) => config,
        None => {
            tracing::warn!(
                "Unsupported Contract: {:?} for chaind : {}",
                contract,
                chain_id
            );
            return Ok(warp::reply::with_status(
                warp::reply::json(&UnsupportedFeature {
                    message: format!(
                        "Unsupported Contract: {} for chaind : {}",
                        contract, chain_id
                    ),
                }),
                warp::http::StatusCode::BAD_REQUEST,
            ));
        }
    };
    // check if data query is enabled for contract
    if !event_watcher_config.enable_data_query {
        tracing::warn!("Enbable data query for contract : ({})", contract);
        return Ok(warp::reply::with_status(
            warp::reply::json(&UnsupportedFeature {
                message: format!(
                    "Enbable data query for contract : ({})",
                    contract
                ),
            }),
            warp::http::StatusCode::FORBIDDEN,
        ));
    }
    // create history store key
    let src_target_system =
        TargetSystem::new_contract_address(contract.to_fixed_bytes());
    let src_typed_chain_id = TypedChainId::Evm(chain_id);
    let history_store_key =
        ResourceId::new(src_target_system, src_typed_chain_id);
    let encrypted_output =
        store.get_encrypted_output(history_store_key).unwrap();
    let last_queried_block = store
        .get_last_deposit_block_number_for_encrypted_output(history_store_key)
        .unwrap();

    Ok(warp::reply::with_status(
        warp::reply::json(&EncryptedOutputsCacheResponse {
            encrypted_outputs: encrypted_output,
            last_queried_block,
        }),
        warp::http::StatusCode::OK,
    ))
}

/// Handles relayer metric requests
///
/// Returns a Result with the `MetricResponse` on success
pub async fn handle_metric_info() -> Result<impl warp::Reply, Infallible> {
    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct RelayerMetricResponse {
        metrics: String,
    }

    let metric_gathered = Metrics::gather_metrics();
    Ok(warp::reply::with_status(
        warp::reply::json(&RelayerMetricResponse {
            metrics: metric_gathered,
        }),
        warp::http::StatusCode::OK,
    ))
}

/// Handles the command prompts for EVM and Substrate chains
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `cmd` - The command to execute
/// * `stream` - The stream to write the response to
pub async fn handle_cmd(
    ctx: RelayerContext,
    cmd: Command,
    stream: CommandStream,
) {
    use CommandResponse::*;
    if ctx.config.features.private_tx_relay {
        match cmd {
            Command::Substrate(sub) => handle_substrate(ctx, sub, stream).await,
            Command::Evm(evm) => handle_evm(ctx, evm, stream).await,
            Command::Ping() => {
                let _ = stream.send(Pong()).await;
            }
        }
    } else {
        tracing::error!("Private transaction relaying is not configured..!");
        let _ = stream
            .send(Error(
                "Private transaction relaying is not enabled.".to_string(),
            ))
            .await;
    }
}

/// Handler for EVM commands
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `cmd` - The command to execute
/// * `stream` - The stream to write the response to
pub async fn handle_evm(
    ctx: RelayerContext,
    cmd: EvmCommand,
    stream: CommandStream,
) {
    if let CommandType::VAnchor(_) = cmd {
        handle_vanchor_relay_tx(ctx, cmd, stream).await
    }
}

/// Handler for Substrate commands
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `cmd` - The command to execute
/// * `stream` - The stream to write the response to
pub async fn handle_substrate<'a>(
    ctx: RelayerContext,
    cmd: SubstrateCommand,
    stream: CommandStream,
) {
    match cmd {
        CommandType::Mixer(_) => {
            handle_substrate_mixer_relay_tx(ctx, cmd, stream).await;
        }
        CommandType::VAnchor(_) => {
            handle_substrate_vanchor_relay_tx(ctx, cmd, stream).await;
        }
    }
}
