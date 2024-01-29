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
/// Module for all the default values.
pub mod defaults;
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

use ethereum_types::Address;
use evm::EvmChainConfig;
use serde::{Deserialize, Serialize};
use signing_backend::ProposalSigningBackendConfig;
use std::collections::{HashMap, HashSet};
use substrate::SubstrateConfig;
use webb::evm::ethers::types::Chain;
use webb_relayer_types::etherscan_api::EtherscanApiKey;

/// WebbRelayerConfig is the configuration for the webb relayer.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(rename_all(serialize = "camelCase", deserialize = "kebab-case"))]
pub struct WebbRelayerConfig {
    /// WebSocket Server Port number
    ///
    /// default to 9955
    #[serde(default = "defaults::relayer_port", skip_serializing)]
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
    #[serde(default = "defaults::unlisted_assets")]
    pub assets: HashMap<String, UnlistedAssetConfig>,
    /// The type of the optional signing backend used for signing proposals. It can be None for pure Tx relayers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposal_signing_backend: Option<ProposalSigningBackendConfig>,
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

/// FeaturesConfig is the configuration for running relayer with option.
#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "kebab-case"))]
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

/// Configuration to add etherscan API key
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "kebab-case"))]
pub struct EtherscanApiConfig {
    /// Chain Id
    pub chain_id: u32,
    /// A wrapper type around the `String` to allow reading it from the env.
    #[serde(skip_serializing)]
    pub api_key: EtherscanApiKey,
    /// An optional URL to use for the Etherscan API instead of the default.
    ///
    /// This is useful for testing against a local Etherscan API.
    /// Or in case of testnets, the Etherscan GasOracle API is not available.
    /// So we can use the mainnet API URL to get the gas price.
    pub api_url: Option<url::Url>,
}

/// TxQueueConfig is the configuration for the TxQueue.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "kebab-case"))]
pub struct TxQueueConfig {
    /// Maximum number of milliseconds to wait before dequeuing a transaction from
    /// the queue.
    pub max_sleep_interval: u64,
    /// Polling interval in milliseconds to wait before checking pending tx state on chain.
    pub polling_interval: u64,
}

impl Default for TxQueueConfig {
    fn default() -> Self {
        Self {
            max_sleep_interval: 10_000,
            polling_interval: 12_000,
        }
    }
}

/// UnlistedAssetConfig is the configuration for the assets that are not listed on any exchange.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "kebab-case"))]
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
    #[ignore]
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
                .filter_map(Result::ok)
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
