// Copyright (C) 2022-2024 Webb Technologies Inc.
//
// Tangle is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Tangle is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should receive a copy of the GNU General Public License
// If not, see <http://www.gnu.org/licenses/>.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use ethereum_types::Address;
use serde::Serialize;
use std::sync::Arc;
use webb_proposals::{ResourceId, TargetSystem, TypedChainId};
use webb_relayer_context::RelayerContext;
use webb_relayer_utils::metric::Metrics;
use webb_relayer_utils::HandlerError;

/// Response with resource metrics data
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceMetricResponse {
    /// Total gas spent on Resource.
    pub total_gas_spent: String,
    /// Total fees earned on Resource.
    pub total_fee_earned: String,
    /// Account Balance
    pub account_balance: String,
}

/// Handles relayer metric requests
///
/// Returns a Result with the `MetricResponse` on success
pub async fn handle_metric_info() -> Result<String, HandlerError> {
    let metric_gathered = Metrics::gather_metrics().map_err(|e| {
        HandlerError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
    })?;
    Ok(metric_gathered)
}

/// Handles relayer metric requests for evm based resource
///
/// Returns a Result with the `ResourceMetricResponse` on success
pub async fn handle_evm_metric_info(
    State(ctx): State<Arc<RelayerContext>>,
    Path((chain_id, contract)): Path<(u32, Address)>,
) -> Json<ResourceMetricResponse> {
    let mut metrics = ctx.metrics.lock().await;
    // create resource_id for evm target system
    let target_system =
        TargetSystem::new_contract_address(contract.to_fixed_bytes());
    let typed_chain_id = TypedChainId::Evm(chain_id);
    let resource_id = ResourceId::new(target_system, typed_chain_id);
    // fetch metric for given resource_id
    let account_balance = metrics
        .account_balance_entry(typed_chain_id)
        .get()
        .to_string();
    let resource_metric = metrics.resource_metric_entry(resource_id);

    Json(ResourceMetricResponse {
        total_gas_spent: resource_metric.total_gas_spent.get().to_string(),
        total_fee_earned: resource_metric.total_fee_earned.get().to_string(),
        account_balance,
    })
}
