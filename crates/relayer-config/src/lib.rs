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

#![warn(missing_docs)]

//! # Relayer Configuration Module 🕸️
//!
//! A module for configuring the relayer.
//!
//! ## Overview
//!
//! The relayer configuration module is responsible for configuring the relayer.
//! Possible configuration include:
//! * `port`: The port the relayer will listen on. Defaults to 9955
//! * `evm`: EVM based networks and the configuration. See [config/config-6sided-eth-bridge](./config/config-6sided-eth-bridge)
//! for an example.
//! * `substrate`: Substrate based networks and the configuration. See [config/local-substrate](./config/local-substrate) for an example.

/// Generic anchor configuration
pub mod anchor;
/// Block poller configuration
pub mod block_poller;
/// CLI configuration
#[cfg(feature = "cli")]
pub mod cli;
/// Event watcher configuration
pub mod event_watcher;
/// EVM configuration
pub mod evm;
/// Signing backend configuration
pub mod signing_backend;
/// Substrate configuration
pub mod substrate;
/// Utils for processing configuration
pub mod utils;

use evm::EvmChainConfig;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use substrate::SubstrateConfig;

/// The default port the relayer will listen on. Defaults to 9955.
const fn default_port() -> u16 {
    9955
}
/// Leaves watcher is set to `true` by default.
const fn enable_leaves_watcher_default() -> bool {
    true
}
/// Data query access is set to `true` by default.
const fn enable_data_query_default() -> bool {
    true
}
/// The maximum events per step is set to `100` by default.
const fn max_blocks_per_step_default() -> u64 {
    100
}
/// The print progress interval is set to `7_000` by default.
const fn print_progress_interval_default() -> u64 {
    7_000
}

/// WebbRelayerConfig is the configuration for the webb relayer.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct WebbRelayerConfig {
    /// WebSocket Server Port number
    ///
    /// default to 9955
    #[serde(default = "default_port", skip_serializing)]
    pub port: u16,
    /// EVM based networks and the configuration.
    ///
    /// a map between chain name and its configuration.
    #[serde(default)]
    pub evm: HashMap<String, EvmChainConfig>,
    /// ETH2 based networks and the configuration
    /// 
    /// a map between chain name and its configuration
    #[cfg(feature = "eth2")]
    #[serde(default)]
    pub eth2: HashMap<String, eth2_to_substrate_relay::config::Config>,
    /// Substrate based networks and the configuration.
    ///
    /// a map between chain name and its configuration.
    #[serde(default)]
    pub substrate: HashMap<String, SubstrateConfig>,
    /// For Experimental Options
    #[serde(default)]
    pub experimental: ExperimentalConfig,
    /// Configuration for running relayer
    ///
    /// by deafult all features are enabled
    /// Features:
    /// 1. Data quering for leafs
    /// 2. Governance relaying
    /// 3. Private transaction relaying
    #[serde(default)]
    pub features: FeaturesConfig,
}

impl WebbRelayerConfig {
    /// Makes sure that the config is valid, by going
    /// through the whole config and doing some basic checks.
    #[allow(unused)] // TODO(@shekohex): remove this once we convert the relayer into a crate.
    pub fn verify(&self) -> webb_relayer_utils::Result<()> {
        // The first check is to make sure that the private key is there when needed.
        // to say more on the above check, we **must** have a private key in the following conditions:
        // 1. We are running the relayer as a private transaction relayer.
        // 2. we are running the relayer as a governance system.
        //
        // However, if we are running the relayer as only a data serving relayer, we don't need a private key.
        let check_features =
            self.features.governance_relay || self.features.private_tx_relay;
        let check_evm = check_features
            && self
                .evm
                .iter()
                .filter(|(_k, v)| v.enabled)
                .all(|(_k, v)| v.private_key.is_some());
        let check_substrate = check_features
            && self
                .substrate
                .iter()
                .filter(|(_k, v)| v.enabled)
                .all(|(_k, v)| v.suri.is_some());
        (check_evm && check_substrate)
            .then_some(())
            .ok_or(webb_relayer_utils::Error::MissingSecrets)
    }
}

/// ExperimentalConfig is the configuration for the Experimental Options.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct ExperimentalConfig {
    /// Enable the Smart Anchor Updates when it comes to signaling
    /// the bridge to create the proposals.
    pub smart_anchor_updates: bool,
    /// The number of retries to check if an anchor is updated before sending our update
    /// or not, before actually sending our update.
    pub smart_anchor_updates_retries: u32,
}

/// FeaturesConfig is the configuration for running relayer with option.
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct FeaturesConfig {
    /// Enable data quering for leafs
    pub data_query: bool,
    /// Enable governance relaying
    pub governance_relay: bool,
    /// Enable private tx relaying
    pub private_tx_relay: bool,
}
impl Default for FeaturesConfig {
    fn default() -> Self {
        Self {
            data_query: true,
            governance_relay: true,
            private_tx_relay: true,
        }
    }
}

/// TxQueueConfig is the configuration for the TxQueue.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TxQueueConfig {
    /// Maximum number of milliseconds to wait before dequeuing a transaction from
    /// the queue.
    pub max_sleep_interval: u64,
}

impl Default for TxQueueConfig {
    fn default() -> Self {
        Self {
            max_sleep_interval: 10_000,
        }
    }
}
