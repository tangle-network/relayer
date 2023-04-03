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
use webb::evm::ethers::types::Chain;
use webb_relayer_types::etherscan_api::EtherscanApiKey;

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

/// The default unlisted assets.
fn default_unlisted_assets() -> HashMap<String, UnlistedAssetConfig> {
    HashMap::from_iter([
        (
            String::from("tTNT"),
            UnlistedAssetConfig {
                name: String::from("Test Tangle Network Token"),
                decimals: 18,
                price: 0.10,
            },
        ),
        (
            String::from("TNT"),
            UnlistedAssetConfig {
                name: String::from("Tangle Network Token"),
                decimals: 18,
                price: 0.10,
            },
        ),
    ])
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
    /// Etherscan API key configuration for evm based chains.
    #[serde(default, skip_serializing)]
    pub evm_etherscan: HashMap<Chain, EtherscanApiConfig>,
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
    /// Configuration for the assets that are not listed on any exchange.
    ///
    /// it is a simple map between the asset symbol and its configuration.
    #[serde(default = "default_unlisted_assets")]
    pub assets: HashMap<String, UnlistedAssetConfig>,
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
    #[serde(rename(serialize = "smartAnchorUpdates"))]
    pub smart_anchor_updates: bool,
    /// The number of retries to check if an anchor is updated before sending our update
    /// or not, before actually sending our update.
    #[serde(rename(serialize = "smartAnchorUpdatesRetries"))]
    pub smart_anchor_updates_retries: u32,
}

/// FeaturesConfig is the configuration for running relayer with option.
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct FeaturesConfig {
    /// Enable data quering for leafs
    #[serde(rename(serialize = "dataQuery"))]
    pub data_query: bool,
    /// Enable governance relaying
    #[serde(rename(serialize = "governanceRelay"))]
    pub governance_relay: bool,
    /// Enable private tx relaying
    #[serde(rename(serialize = "privateTxRelay"))]
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

/// Configuration to add etherscan API key
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct EtherscanApiConfig {
    /// Chain Id
    #[serde(rename(serialize = "chainId"))]
    pub chain_id: u32,
    /// A wrapper type around the `String` to allow reading it from the env.
    #[serde(skip_serializing)]
    pub api_key: EtherscanApiKey,
}

/// TxQueueConfig is the configuration for the TxQueue.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct TxQueueConfig {
    /// Maximum number of milliseconds to wait before dequeuing a transaction from
    /// the queue.
    #[serde(rename(serialize = "maxSleepInterval"))]
    pub max_sleep_interval: u64,
}

impl Default for TxQueueConfig {
    fn default() -> Self {
        Self {
            max_sleep_interval: 10_000,
        }
    }
}

/// UnlistedAssetConfig is the configuration for the assets that are not listed on any exchange.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct UnlistedAssetConfig {
    /// The Price of the asset in USD.
    pub price: f64,
    /// The name of the asset.
    pub name: String,
    /// The decimals of the asset.
    pub decimals: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_config_files_are_correct() {
        // This test is to make sure that all the config files are correct.
        // This walks all the directories inside the root of the config directory
        // and tries to parse the config file(s) inside it.

        let git_root = std::process::Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .expect("Failed to get git root")
            .stdout;
        let git_root = std::str::from_utf8(&git_root)
            .expect("Failed to parse git root")
            .trim();
        let config_dir = std::path::Path::new(git_root).join("config");
        let config_dirs =
            glob::glob(config_dir.join("**").join("**").to_str().unwrap())
                .expect("Failed to read config directory")
                .filter_map(|p| p.ok())
                .filter(|p| p.is_dir())
                .collect::<Vec<_>>();
        assert!(
            !config_dirs.is_empty(),
            "No config directories found in the config directory"
        );
        let cwd =
            std::env::current_dir().expect("Failed to get current directory");
        // For each config directory, we try to parse the config file(s) inside it.
        for config_subdir in config_dirs {
            std::env::set_current_dir(&config_subdir)
                .expect("Failed to set current directory");
            // Load the example dot env file.
            let _ = dotenv::from_filename(".env.example");
            if let Err(e) = utils::load(&config_subdir) {
                panic!("Failed to parse config file in directory: {config_subdir:?} with error: {e}");
            }

            dotenv::vars().for_each(|(k, _)| {
                std::env::remove_var(k);
            });

            std::env::set_current_dir(&cwd)
                .expect("Failed to set current directory");
        }
    }
}
