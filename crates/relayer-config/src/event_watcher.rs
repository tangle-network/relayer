use super::*;

/// EventsWatchConfig is the configuration for the events watch.
#[derive(Debug, Clone, Serialize, Deserialize, Default, Copy)]
#[serde(rename_all(serialize = "camelCase", deserialize = "kebab-case"))]
pub struct EventsWatcherConfig {
    /// A flag for enabling API endpoints for querying data from the relayer.
    #[serde(default = "defaults::enable_data_query")]
    pub enable_data_query: bool,
    #[serde(default = "defaults::enable_leaves_watcher")]
    /// if it is enabled for this chain or not.
    pub enabled: bool,
    /// Polling interval in milliseconds
    pub polling_interval: u64,
    /// The maximum number of events to fetch in one request.
    #[serde(default = "defaults::max_blocks_per_step")]
    pub max_blocks_per_step: u64,
    /// print sync progress frequency in milliseconds
    /// if it is zero, means no progress will be printed.
    #[serde(default = "defaults::print_progress_interval")]
    pub print_progress_interval: u64,
    /// Sync blocks from
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_blocks_from: Option<u64>,
}
