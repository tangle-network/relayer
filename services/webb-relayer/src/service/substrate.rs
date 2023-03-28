use std::sync::Arc;

use axum::routing::get;
use axum::Router;
use webb_relayer_context::RelayerContext;
use webb_relayer_handlers::routes::{leaves, metric};

/// Setup and build all the Substrate web services and handlers.
pub fn build_web_services() -> Router<Arc<RelayerContext>> {
    Router::new()
        .route(
            "/leaves/substrate/:chain_id/:tree_id/:pallet_id",
            get(leaves::handle_leaves_cache_substrate),
        )
        .route(
            "/metrics/substrate/:chain_id/:tree_id/:pallet_id",
            get(metric::handle_substrate_metric_info),
        )
}

pub async fn ignite(
    ctx: &RelayerContext,
    store: Arc<super::Store>,
) -> crate::Result<()> {
    todo!("Ignite Substrate relayer")
}
