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
