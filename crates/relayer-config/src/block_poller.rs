use super::*;
use webb_relayer_types::rpc_url::RpcUrl;

/// Block poller configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct BlockPollerConfig {
    /// The starting block to listen at.
    #[serde(default)]
    pub start_block: Option<u64>,
    /// Polling interval in milliseconds
    #[serde(rename(serialize = "pollingInterval"))]
    pub polling_interval: u64,
    /// The maximum blocks per step.
    ///
    /// default to 100
    #[serde(default = "max_blocks_per_step_default")]
    pub max_blocks_per_step: u64,
    /// The print progress interval.
    ///
    /// default to 7_000
    #[serde(default = "print_progress_interval_default")]
    pub print_progress_interval: u64,
    /// Light client RPC url
    #[serde(default)]
    pub light_client_rpc_url: Option<RpcUrl>,
}
