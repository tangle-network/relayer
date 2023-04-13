use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use ethereum_types::Address;
use serde::Serialize;
use std::sync::Arc;
use webb_proposals::{
    ResourceId, SubstrateTargetSystem, TargetSystem, TypedChainId,
};
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
    let resource_metric = metrics
        .resource_metric_map
        .entry(resource_id)
        .or_insert_with(|| Metrics::register_resource_id_counters(resource_id));

    Json(ResourceMetricResponse {
        total_gas_spent: resource_metric.total_gas_spent.get().to_string(),
        total_fee_earned: resource_metric.total_fee_earned.get().to_string(),
        account_balance: resource_metric.account_balance.get().to_string(),
    })
}

/// Handles relayer metric requests for substrate based resource
///
/// Returns a Result with the `ResourceMetricResponse` on success
pub async fn handle_substrate_metric_info(
    State(ctx): State<Arc<RelayerContext>>,
    Path((chain_id, tree_id, pallet_id)): Path<(u32, u32, u8)>,
) -> Json<ResourceMetricResponse> {
    let mut metrics = ctx.metrics.lock().await;
    // create resource_id for substrate target system
    let target = SubstrateTargetSystem::builder()
        .pallet_index(pallet_id)
        .tree_id(tree_id)
        .build();
    let target_system = TargetSystem::Substrate(target);
    let typed_chain_id = TypedChainId::Substrate(chain_id);
    let resource_id = ResourceId::new(target_system, typed_chain_id);

    // fetch metric for given resource_id
    let resource_metric = metrics
        .resource_metric_map
        .entry(resource_id)
        .or_insert_with(|| Metrics::register_resource_id_counters(resource_id));

    Json(ResourceMetricResponse {
        total_gas_spent: resource_metric.total_gas_spent.get().to_string(),
        total_fee_earned: resource_metric.total_fee_earned.get().to_string(),
        account_balance: resource_metric.account_balance.get().to_string(),
    })
}
