use super::*;
use webb_relayer_types::rpc_url::RpcUrl;

/// Block poller configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "kebab-case"))]
pub struct BlockPollerConfig {
    /// The starting block to listen at.
    #[serde(default)]
    pub start_block: Option<u64>,
    /// Polling interval in milliseconds
    pub polling_interval: u64,
    /// The maximum blocks per step.
    ///
    /// default to 100
    #[serde(default = "defaults::max_blocks_per_step")]
    pub max_blocks_per_step: u64,
    /// The print progress interval.
    ///
    /// default to 7_000
    #[serde(default = "defaults::print_progress_interval")]
    pub print_progress_interval: u64,
    /// Light client RPC url
    #[serde(default)]
    pub light_client_rpc_url: Option<RpcUrl>,
}

impl Default for BlockPollerConfig {
    fn default() -> Self {
        Self {
            start_block: None,
            polling_interval: 6000,
            max_blocks_per_step: defaults::max_blocks_per_step(),
            print_progress_interval: defaults::print_progress_interval(),
            light_client_rpc_url: None,
        }
    }
}
