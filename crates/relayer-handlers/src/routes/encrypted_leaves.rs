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

use ethereum_types::Address;
use serde::Serialize;
use std::{collections::HashMap, convert::Infallible, sync::Arc};
use webb_proposals::{ResourceId, TargetSystem, TypedChainId};
use webb_relayer_context::RelayerContext;
use webb_relayer_store::EncryptedOutputCacheStore;

use crate::routes::UnsupportedFeature;

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
            webb_relayer_config::evm::Contract::VAnchor(c)
            | webb_relayer_config::evm::Contract::OpenVAnchor(c) => {
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
