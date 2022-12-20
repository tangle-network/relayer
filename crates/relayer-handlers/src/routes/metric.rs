use serde::Serialize;
use std::convert::Infallible;

use webb_relayer_utils::metric::Metrics;

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
