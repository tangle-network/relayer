use super::*;

/// EventsWatchConfig is the configuration for the events watch.
#[derive(Debug, Clone, Serialize, Deserialize, Default, Copy)]
#[serde(rename_all = "kebab-case")]
pub struct EventsWatcherConfig {
    /// A flag for enabling API endpoints for querying data from the relayer.
    #[serde(default = "enable_data_query_default")]
    pub enable_data_query: bool,
    #[serde(default = "enable_leaves_watcher_default")]
    /// if it is enabled for this chain or not.
    pub enabled: bool,
    /// Polling interval in milliseconds
    #[serde(rename(serialize = "pollingInterval"))]
    pub polling_interval: u64,
    /// The maximum number of events to fetch in one request.
    #[serde(skip_serializing, default = "max_blocks_per_step_default")]
    pub max_blocks_per_step: u64,
    /// print sync progress frequency in milliseconds
    /// if it is zero, means no progress will be printed.
    #[serde(skip_serializing, default = "print_progress_interval_default")]
    pub print_progress_interval: u64,
    /// Sync block from
    #[serde(rename(serialize = "syncBlocksFrom"))]
    pub sync_blocks_form: Option<u64>,
}
