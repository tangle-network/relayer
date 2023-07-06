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

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use ethereum_types::Address;
use serde::Serialize;
use std::{collections::HashMap, sync::Arc};
use webb_proposals::{ResourceId, TargetSystem, TypedChainId};
use webb_relayer_context::RelayerContext;
use webb_relayer_store::EncryptedOutputCacheStore;
use webb_relayer_utils::HandlerError;

use super::OptionalRangeQuery;

/// Response containing encrypted outputs.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptedOutputsCacheResponse {
    encrypted_outputs: Vec<String>,
    last_queried_block: u64,
}

/// Handles encrypted outputs data requests for evm
///
/// Returns a Result with the `EncryptedOutputDataResponse` on success
///
/// # Arguments
///
/// * `chain_id` - An U256 representing the chain id of the chain to query
/// * `contract` - An address of the contract to query
/// * `query_range` - An optional range query
pub async fn handle_encrypted_outputs_cache_evm(
    State(ctx): State<Arc<RelayerContext>>,
    Path((chain_id, contract)): Path<(u32, Address)>,
    Query(query_range): Query<OptionalRangeQuery>,
) -> Result<Json<EncryptedOutputsCacheResponse>, HandlerError> {
    let config = ctx.config.clone();

    // check if data query is enabled for relayer
    if !config.features.data_query {
        tracing::warn!("Data query is not enabled for relayer.");
        return Err(HandlerError(
            StatusCode::FORBIDDEN,
            "Data query is not enabled for relayer.".to_string(),
        ));
    }

    // check if chain is supported
    let chain = match ctx.config.evm.get(&chain_id.to_string()) {
        Some(v) => v,
        None => {
            tracing::warn!("Unsupported Chain: {chain_id}");
            return Err(HandlerError(
                StatusCode::BAD_REQUEST,
                format!("Unsupported Chain: {chain_id}"),
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
                "Unsupported Contract: {contract} for chaind : {chain_id}",
            );
            return Err(HandlerError(
                StatusCode::BAD_REQUEST,
                format!(
                    "Unsupported Contract: {contract} for chaind : {chain_id}"
                ),
            ));
        }
    };
    // check if data query is enabled for contract
    if !event_watcher_config.enable_data_query {
        tracing::warn!("Enbable data query for contract : ({})", contract);
        return Err(HandlerError(
            StatusCode::FORBIDDEN,
            format!("Enbable data query for contract : ({contract})"),
        ));
    }
    // create history store key
    let src_target_system =
        TargetSystem::new_contract_address(contract.to_fixed_bytes());
    let src_typed_chain_id = TypedChainId::Evm(chain_id);
    let history_store_key =
        ResourceId::new(src_target_system, src_typed_chain_id);
    let encrypted_output = ctx.store().get_encrypted_output_with_range(
        history_store_key,
        query_range.into(),
    )?;
    let last_queried_block = ctx
        .store()
        .get_last_deposit_block_number_for_encrypted_output(
            history_store_key,
        )?;

    Ok(Json(EncryptedOutputsCacheResponse {
        encrypted_outputs: encrypted_output,
        last_queried_block,
    }))
}
