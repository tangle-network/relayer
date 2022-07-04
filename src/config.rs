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
//

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
//!
//! Checkout [config](./config) for useful default configurations for many networks.
//! These config files can be changed to your preferences.
use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;

use ethereum_types::{Address, Secret, U256};
use serde::{Deserialize, Serialize};
use webb::substrate::subxt::sp_core::sr25519::{Pair as Sr25519Pair, Public};
use webb::substrate::subxt::sp_core::Pair;

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
const fn max_events_per_step_default() -> u64 {
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
/// EvmChainConfig is the configuration for the EVM based networks.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct EvmChainConfig {
    /// Boolean indicating EVM based networks are enabled or not.
    #[serde(default)]
    pub enabled: bool,
    /// Http(s) Endpoint for quick Req/Res
    #[serde(skip_serializing)]
    pub http_endpoint: url::Url,
    /// Websocket Endpoint for long living connections
    #[serde(skip_serializing)]
    pub ws_endpoint: url::Url,
    /// Block Explorer for this chain.
    ///
    /// Optional, and only used for printing a clickable links
    /// for transactions and contracts.
    #[serde(skip_serializing)]
    pub explorer: Option<url::Url>,
    /// chain specific id.
    #[serde(rename(serialize = "chainId"))]
    pub chain_id: u64,
    /// The Private Key of this account on this network
    /// the format is more dynamic here:
    /// 1. if it starts with '0x' then this would be raw (64 bytes) hex encoded
    ///    private key.
    ///    Example: 0x8917174396171783496173419137618235192359106130478137647163400318
    ///
    /// 2. if it starts with '$' then it would be considered as an Enviroment variable
    ///    of a hex-encoded private key.
    ///   Example: $HARMONY_PRIVATE_KEY
    ///
    /// 3. if it starts with '> ' then it would be considered as a command that
    ///   the relayer would execute and the output of this command would be the
    ///   hex encoded private key.
    ///   Example: > pass harmony-privatekey
    ///
    /// 4. if it doesn't contains special characters and has 12 or 24 words in it
    ///   then we should process it as a mnemonic string: 'word two three four ...'
    #[serde(skip_serializing)]
    pub private_key: PrivateKey,
    /// Optionally, a user can specify an account to receive rewards for relaying
    pub beneficiary: Option<Address>,
    /// Supported contracts over this chain.
    #[serde(default)]
    pub contracts: Vec<Contract>,
    /// TxQueue configuration
    #[serde(skip_serializing, default)]
    pub tx_queue: TxQueueConfig,
}
/// SubstrateConfig is the configuration for the Substrate based networks.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct SubstrateConfig {
    /// Boolean indicating Substrate networks are enabled or not.
    #[serde(default)]
    pub enabled: bool,
    /// Http(s) Endpoint for quick Req/Res
    #[serde(skip_serializing)]
    pub http_endpoint: url::Url,
    /// Websocket Endpoint for long living connections
    #[serde(skip_serializing)]
    pub ws_endpoint: url::Url,
    /// Block Explorer for this Substrate node.
    ///
    /// Optional, and only used for printing a clickable links
    /// for transactions and contracts.
    #[serde(skip_serializing)]
    pub explorer: Option<url::Url>,
    /// Interprets the string in order to generate a key Pair. in the
    /// case that the pair can be expressed as a direct derivation from a seed (some cases, such as Sr25519 derivations
    /// with path components, cannot).
    ///
    /// This takes a helper function to do the key generation from a phrase, password and
    /// junction iterator.
    ///
    /// - If `s` begins with a `$` character it is interpreted as an environment variable.
    /// - If `s` is a possibly `0x` prefixed 64-digit hex string, then it will be interpreted
    /// directly as a `MiniSecretKey` (aka "seed" in `subkey`).
    /// - If `s` is a valid BIP-39 key phrase of 12, 15, 18, 21 or 24 words, then the key will
    /// be derived from it. In this case:
    ///   - the phrase may be followed by one or more items delimited by `/` characters.
    ///   - the path may be followed by `///`, in which case everything after the `///` is treated
    /// as a password.
    /// - If `s` begins with a `/` character it is prefixed with the Substrate public `DEV_PHRASE` and
    /// interpreted as above.
    ///
    /// In this case they are interpreted as HDKD junctions; purely numeric items are interpreted as
    /// integers, non-numeric items as strings. Junctions prefixed with `/` are interpreted as soft
    /// junctions, and with `//` as hard junctions.
    ///
    /// There is no correspondence mapping between SURI strings and the keys they represent.
    /// Two different non-identical strings can actually lead to the same secret being derived.
    /// Notably, integer junction indices may be legally prefixed with arbitrary number of zeros.
    /// Similarly an empty password (ending the SURI with `///`) is perfectly valid and will generally
    /// be equivalent to no password at all.
    ///
    /// `None` is returned if no matches are found.
    #[serde(skip_serializing)]
    pub suri: Suri,
    /// Optionally, a user can specify an account to receive rewards for relaying
    pub beneficiary: Option<Public>,
    /// Which Substrate Runtime to use?
    pub runtime: SubstrateRuntime,
    /// Supported pallets over this substrate node.
    #[serde(default)]
    pub pallets: Vec<Pallet>,
}
/// ExperimentalConfig is the configuration for the Experimental Options.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default)]
#[serde(rename_all = "kebab-case")]
pub struct ExperimentalConfig {
    /// Enable the Smart Anchor Updates when it comes to signaling
    /// the bridge to create the proposals.
    pub smart_anchor_updates: bool,
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
/// EventsWatchConfig is the configuration for the events watch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct EventsWatcherConfig {
    #[serde(default = "enable_data_query_default")]
    pub enable_data_query: bool,
    #[serde(default = "enable_leaves_watcher_default")]
    /// if it is enabled for this chain or not.
    pub enabled: bool,
    /// Polling interval in milliseconds
    #[serde(rename(serialize = "pollingInterval"))]
    pub polling_interval: u64,
    /// The maximum number of events to fetch in one request.
    #[serde(skip_serializing, default = "max_events_per_step_default")]
    pub max_events_per_step: u64,
    /// print sync progress frequency in milliseconds
    /// if it is zero, means no progress will be printed.
    #[serde(skip_serializing, default = "print_progress_interval_default")]
    pub print_progress_interval: u64,
}

/// AnchorWithdrawConfig is the configuration for the Anchor Withdraw.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AnchorWithdrawConfig {
    /// The fee percentage that your account will receive when you relay a transaction
    /// over this chain.
    #[serde(rename(serialize = "withdrawFeePercentage"))]
    pub withdraw_fee_percentage: f64,
    /// A hex value of the gaslimit when doing a withdraw relay transaction on this chain.
    #[serde(rename(serialize = "withdrawGaslimit"))]
    pub withdraw_gaslimit: U256,
}

/// LinkedAnchorConfig is the configuration for the linked anchor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LinkedAnchorConfig {
    /// The Chain name where this anchor belongs to.
    /// and it is case-insensitive.
    pub chain: String,
    /// The Anchor Contract Address.
    pub address: Address,
}

/// SubstrateLinkedAnchorConfig is the configuration for the linked anchor of substrate.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SubstrateLinkedAnchorConfig {
    /// The Chain Id where this anchor belongs to.
    pub chain: u32,
    /// Tree Id of the anchor
    #[serde(rename(serialize = "tree"))]
    pub tree: u32,
}

/// Enumerates the supported contract configurations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "contract")]
pub enum Contract {
    Anchor(AnchorContractConfig),
    VAnchor(VAnchorContractConfig),
    SignatureBridge(SignatureBridgeContractConfig),
    GovernanceBravoDelegate(GovernanceBravoDelegateContractConfig),
}

/// Enumerates the supported pallets configurations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "pallet")]
pub enum Pallet {
    #[serde(rename = "DKG")]
    Dkg(DKGPalletConfig),
    DKGProposals(DKGProposalsPalletConfig),
    DKGProposalHandler(DKGProposalHandlerPalletConfig),
    AnchorBn254(AnchorBn254PalletConfig),
    SignatureBridge(SignatureBridgePalletConfig),
    VAnchorBn254(VAnchorBn254PalletConfig),
}

/// Enumerates the supported Substrate runtimes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SubstrateRuntime {
    #[serde(rename = "DKG")]
    Dkg,
    WebbProtocol,
}

/// CommonContractConfig represents the common configuration for contracts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CommonContractConfig {
    /// The address of this contract on this chain.
    pub address: Address,
    /// the block number where this contract got deployed at.
    #[serde(rename(serialize = "deployedAt"))]
    pub deployed_at: u64,
}

/// AnchorContractOverDKGConfig represents the configuration for the Anchor contract over DKG.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AnchorContractConfig {
    #[serde(flatten)]
    pub common: CommonContractConfig,
    /// Controls the events watcher
    #[serde(rename(serialize = "eventsWatcher"))]
    pub events_watcher: EventsWatcherConfig,
    /// The size of this contract
    pub size: f64,
    /// Anchor withdraw configuration.
    #[serde(rename(serialize = "withdrawConfig"))]
    pub withdraw_config: Option<AnchorWithdrawConfig>,
    /// The type of the optional signing backend used for signing proposals. It can be None for pure Tx relayers
    #[serde(rename(serialize = "proposalSigningBackend"))]
    pub proposal_signing_backend: Option<ProposalSigningBackendConfig>,
    /// A List of linked Anchor Contracts (on other chains) to this contract.
    #[serde(rename(serialize = "linkedAnchors"), default)]
    pub linked_anchors: Option<Vec<LinkedAnchorConfig>>,
}

/// VAnchorContractConfig represents the configuration for the VAnchor contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct VAnchorContractConfig {
    #[serde(flatten)]
    pub common: CommonContractConfig,
    /// Controls the events watcher
    #[serde(rename(serialize = "eventsWatcher"))]
    pub events_watcher: EventsWatcherConfig,
    /// The size of this contract
    pub size: f64,
    /// Anchor withdraw configuration.
    #[serde(rename(serialize = "withdrawConfig"))]
    pub withdraw_config: Option<AnchorWithdrawConfig>,
    /// The type of the optional signing backend used for signing proposals. It can be None for pure Tx relayers
    #[serde(rename(serialize = "proposalSigningBackend"))]
    pub proposal_signing_backend: Option<ProposalSigningBackendConfig>,
    /// A List of linked Anchor Contracts (on other chains) to this contract.
    #[serde(rename(serialize = "linkedAnchors"), default)]
    pub linked_anchors: Option<Vec<LinkedAnchorConfig>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SignatureBridgeContractConfig {
    #[serde(flatten)]
    pub common: CommonContractConfig,
    /// Controls the events watcher
    #[serde(rename(serialize = "eventsWatcher"))]
    pub events_watcher: EventsWatcherConfig,
}

/// GovernanceBravoDelegateContractConfig represents the configuration for the GovernanceBravoDelegate contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct GovernanceBravoDelegateContractConfig {
    #[serde(flatten)]
    pub common: CommonContractConfig,
    // TODO(@shekohex): add more fields here...
}

/// DKGProposalsPalletConfig represents the configuration for the DKGProposals pallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DKGProposalsPalletConfig {
    /// Controls the events watcher
    #[serde(rename(serialize = "eventsWatcher"))]
    pub events_watcher: EventsWatcherConfig,
}

/// DKGPalletConfig represents the configuration for the DKG pallet (dkg-metadata).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DKGPalletConfig {
    /// Controls the events watcher
    #[serde(rename(serialize = "eventsWatcher"))]
    pub events_watcher: EventsWatcherConfig,
}
/// DKGProposalHandlerPalletConfig represents the configuration for the DKGProposalHandler pallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DKGProposalHandlerPalletConfig {
    /// Controls the events watcher
    #[serde(rename(serialize = "eventsWatcher"))]
    pub events_watcher: EventsWatcherConfig,
}

/// AnchorBn254PalletConfig represents the configuration for the AnchorBn254 pallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AnchorBn254PalletConfig {
    /// Controls the events watcher
    #[serde(rename(serialize = "eventsWatcher"))]
    pub events_watcher: EventsWatcherConfig,
    /// The type of the optional signing backend used for signing proposals. It can be None for pure Tx relayers
    #[serde(rename(serialize = "proposalSigningBackend"))]
    pub proposal_signing_backend: Option<ProposalSigningBackendConfig>,
    ///A List of linked Anchor on this chain.
    #[serde(rename(serialize = "linkedAnchors"), default)]
    pub linked_anchors: Vec<SubstrateLinkedAnchorConfig>,
}

/// SignatureBridgePalletConfig represents the configuration for the SignatureBridge pallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SignatureBridgePalletConfig {
    /// Controls the events watcher
    #[serde(rename(serialize = "eventsWatcher"))]
    pub events_watcher: EventsWatcherConfig,
}

/// VAnchorBn254PalletConfig represents the configuration for the VAnchorBn254 pallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct VAnchorBn254PalletConfig {
    /// Controls the events watcher
    #[serde(rename(serialize = "eventsWatcher"))]
    pub events_watcher: EventsWatcherConfig,
    /// The type of the optional signing backend used for signing proposals. It can be None for pure Tx relayers
    #[serde(rename(serialize = "proposalSigningBackend"))]
    pub proposal_signing_backend: Option<ProposalSigningBackendConfig>,
    /// A List of linked Anchor on this chain.
    #[serde(rename(serialize = "linkedAnchors"), default)]
    pub linked_anchors: Vec<SubstrateLinkedAnchorConfig>,
}

/// Enumerates the supported different signing backends configurations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProposalSigningBackendConfig {
    /// Uses an already running and configured DKG Node for signing proposals.
    #[serde(rename = "DKGNode")]
    DkgNode(DkgNodeProposalSigningBackendConfig),
    /// Uses the Private Key of the current Governor to sign proposals.
    Mocked(MockedProposalSigningBackendConfig),
}

/// DKGNodeSigningBackendConfig represents the configuration for the DKGNode signing backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DkgNodeProposalSigningBackendConfig {
    /// The name of the DKG Node that this contract will use.
    ///
    /// Must be defined in the config.
    pub node: String,
}

/// MockedSigningBackendConfig represents the configuration for the Mocked signing backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct MockedProposalSigningBackendConfig {
    /// The private key of the current Governor.
    #[serde(skip_serializing)]
    pub private_key: PrivateKey,
}

/// PrivateKey represents a private key.
#[derive(Clone)]
pub struct PrivateKey(Secret);

impl std::fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PrivateKey").finish()
    }
}

impl std::ops::Deref for PrivateKey {
    type Target = Secret;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de> Deserialize<'de> for PrivateKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct PrivateKeyVistor;
        impl<'de> serde::de::Visitor<'de> for PrivateKeyVistor {
            type Value = Secret;

            fn expecting(
                &self,
                formatter: &mut std::fmt::Formatter,
            ) -> std::fmt::Result {
                formatter.write_str(
                    "hex string or an env var containing a hex string in it",
                )
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if value.starts_with("0x") {
                    // hex value
                    let maybe_hex = Secret::from_str(value);
                    match maybe_hex {
                        Ok(val) => Ok(val),
                        Err(e) => Err(serde::de::Error::custom(format!("{}\n got {} but expected a 66 string (including the 0x prefix)", e, value.len()))),
                    }
                } else if value.starts_with('$') {
                    // env
                    let var = value.strip_prefix('$').unwrap_or(value);
                    tracing::trace!("Reading {} from env", var);
                    let val = std::env::var(var).map_err(|e| {
                        serde::de::Error::custom(format!(
                            "error while loading this env {}: {}",
                            var, e,
                        ))
                    })?;
                    let maybe_hex = Secret::from_str(&val);
                    match maybe_hex {
                        Ok(val) => Ok(val),
                        Err(e) => Err(serde::de::Error::custom(format!("{}\n expected a 66 chars string (including the 0x prefix) but found {} char", e,  val.len()))),
                    }
                } else if value.starts_with('>') {
                    todo!("Implement command execution to extract the private key")
                } else {
                    todo!("Parse the string as mnemonic seed.")
                }
            }
        }

        let secret = deserializer.deserialize_str(PrivateKeyVistor)?;
        Ok(Self(secret))
    }
}

#[derive(Clone)]
pub struct Suri(Sr25519Pair);

impl std::fmt::Debug for Suri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SubstratePrivateKey").finish()
    }
}

impl From<Suri> for Sr25519Pair {
    fn from(suri: Suri) -> Self {
        suri.0
    }
}

impl std::ops::Deref for Suri {
    type Target = Sr25519Pair;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Suri {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct PrivateKeyVistor;
        impl<'de> serde::de::Visitor<'de> for PrivateKeyVistor {
            type Value = Sr25519Pair;

            fn expecting(
                &self,
                formatter: &mut std::fmt::Formatter,
            ) -> std::fmt::Result {
                formatter.write_str(
                    "hex string, dervation path or an env var containing a hex string in it",
                )
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if value.starts_with('$') {
                    // env
                    let var = value.strip_prefix('$').unwrap_or(value);
                    tracing::trace!("Reading {} from env", var);
                    let val = std::env::var(var).map_err(|e| {
                        serde::de::Error::custom(format!(
                            "error while loading this env {}: {}",
                            var, e,
                        ))
                    })?;
                    let maybe_pair =
                        Sr25519Pair::from_string_with_seed(&val, None);
                    match maybe_pair {
                        Ok((pair, _)) => Ok(pair),
                        Err(e) => {
                            Err(serde::de::Error::custom(format!("{:?}", e)))
                        }
                    }
                } else if value.starts_with('>') {
                    todo!("Implement command execution to extract the private key")
                } else {
                    let maybe_pair =
                        Sr25519Pair::from_string_with_seed(value, None);
                    match maybe_pair {
                        Ok((pair, _)) => Ok(pair),
                        Err(e) => {
                            Err(serde::de::Error::custom(format!("{:?}", e)))
                        }
                    }
                }
            }
        }

        let secret = deserializer.deserialize_str(PrivateKeyVistor)?;
        Ok(Self(secret))
    }
}
/// Load the configuration files and
///
/// Returns `Ok(WebbRelayerConfig)` on success, or `Err(anyhow::Error)` on failure.
///
/// # Arguments
///
/// * `path` - The path to the configuration file
///
/// # Example
///
/// ```
/// let path = "/path/to/config.toml";
/// config::load(path);
/// ```
pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<WebbRelayerConfig> {
    let mut cfg = config::Config::new();
    // A pattern that covers all toml or json files in the config directory and subdirectories.
    let toml_pattern = format!("{}/**/*.toml", path.as_ref().display());
    let json_pattern = format!("{}/**/*.json", path.as_ref().display());
    tracing::trace!(
        "Loading config files from {} and {}",
        toml_pattern,
        json_pattern
    );
    // then get an iterator over all matching files
    let config_files = glob::glob(&toml_pattern)?
        .flatten()
        .chain(glob::glob(&json_pattern)?.flatten());
    let contracts: HashMap<String, Vec<Contract>> = HashMap::new();

    // read through all config files for the first time
    // build up a collection of [contracts]
    for config_file in config_files {
        tracing::trace!("Loading config file: {}", config_file.display());
        // get file extension
        let ext = config_file
            .extension()
            .map(|e| e.to_str().unwrap_or(""))
            .unwrap_or("");
        let format = match ext {
            "toml" => config::FileFormat::Toml,
            "json" => config::FileFormat::Json,
            _ => {
                tracing::warn!("Unknown file extension: {}", ext);
                continue;
            }
        };
        let file = config::File::from(config_file).format(format);
        if let Err(e) = cfg.merge(file) {
            tracing::warn!("Error while loading config file: {} skipping!", e);
            continue;
        }
    }

    // also merge in the environment (with a prefix of WEBB).
    cfg.merge(config::Environment::with_prefix("WEBB").separator("_"))?;
    // and finally deserialize the config and post-process it
    let config: Result<
        WebbRelayerConfig,
        serde_path_to_error::Error<config::ConfigError>,
    > = serde_path_to_error::deserialize(cfg);
    match config {
        Ok(mut c) => {
            // merge in all of the contracts into the config
            for (network_name, network_chain) in c.evm.iter_mut() {
                if let Some(stored_contracts) = contracts.get(network_name) {
                    network_chain.contracts = stored_contracts.clone();
                }
            }

            postloading_process(c)
        }
        Err(e) => {
            tracing::error!("{}", e);
            anyhow::bail!("Error while loading config files")
        }
    }
}

/// The postloading_process exists to validate configuration and standardize
/// the format of the configuration
fn postloading_process(
    mut config: WebbRelayerConfig,
) -> anyhow::Result<WebbRelayerConfig> {
    tracing::trace!("Checking configration sanity ...");
    tracing::trace!("postloaded config: {:?}", config);
    // make all chain names lower case
    // 1. drain everything, and take enabled chains.
    let old_evm = config
        .evm
        .drain()
        .filter(|(_, chain)| chain.enabled)
        .collect::<HashMap<_, _>>();
    // 2. insert them again, as lowercased.
    for (k, v) in old_evm {
        config.evm.insert(k.to_lowercase(), v);
    }
    // do the same for substrate
    let old_substrate = config
        .substrate
        .drain()
        .filter(|(_, chain)| chain.enabled)
        .collect::<HashMap<_, _>>();
    for (k, v) in old_substrate {
        config.substrate.insert(k.to_lowercase(), v);
    }
    // check that all required chains are already present in the config.
    for (chain_name, chain_config) in &config.evm {
        let anchors = chain_config.contracts.iter().filter_map(|c| match c {
            Contract::Anchor(cfg) => Some(cfg),
            _ => None,
        });
        let vanchors = chain_config.contracts.iter().filter_map(|c| match c {
            Contract::Anchor(cfg) => Some(cfg),
            _ => None,
        });
        for anchor in anchors {
            // validate config for data querying
            if config.features.data_query {
                // check if events watcher is enabled
                if !anchor.events_watcher.enabled {
                    tracing::warn!(
                        "!!WARNING!!: In order to enable data querying,
                        event-watcher should also be enabled for ({})",
                        anchor.common.address
                    );
                }
                // check if data-query is enabled in evenst-watcher config
                if !anchor.events_watcher.enable_data_query {
                    tracing::warn!(
                        "!!WARNING!!: In order to enable data querying,
                        enable-data-query in events-watcher config should also be enabled for ({})",
                        anchor.common.address
                    );
                }
            }
            // validate config for governance relaying
            if config.features.governance_relay {
                // check if proposal signing backend is configured
                if anchor.proposal_signing_backend.is_none() {
                    tracing::warn!(
                        "!!WARNING!!: In order to enable governance relaying,
                        proposal-signing-backend should be configured for ({})",
                        anchor.common.address
                    );
                }
                // check if event watchers is enabled
                if !anchor.events_watcher.enabled {
                    tracing::warn!(
                        "!!WARNING!!: In order to enable governance relaying,
                        event-watcher should also be enabled for ({})",
                        anchor.common.address
                    );
                }
                // check if linked anchor is configured
                match &anchor.linked_anchors {
                    None => {
                        tracing::warn!(
                            "!!WARNING!!: In order to enable governance relaying,
                            linked-anchors should also be configured for ({})",
                            anchor.common.address
                        );
                    }
                    Some(linked_anchors) => {
                        if linked_anchors.is_empty() {
                            tracing::warn!(
                                "!!WARNING!!: In order to enable governance relaying,
                                linked-anchors cannot be empty.
                                Please congigure Linked anchors for ({})",
                                anchor.common.address
                            );
                        } else {
                            for linked_anchor in linked_anchors {
                                let chain = linked_anchor.chain.to_lowercase();
                                let chain_defined =
                                    config.evm.contains_key(&chain);
                                if !chain_defined {
                                    tracing::warn!("!!WARNING!!: chain {} is not defined in the config.
                                        which is required by the Anchor Contract ({}) defined on {} chain.
                                        Please, define it manually, to allow the relayer to work properly.",
                                        chain,
                                        anchor.common.address,
                                        chain_name
                                    );
                                }
                            }
                        }
                    }
                }
            }
            // validate config for private transaction relaying
            if config.features.private_tx_relay {
                // check if withdraw fee is configured
                if anchor.withdraw_config.is_none() {
                    tracing::warn!(
                        "!!WARNING!!: In order to enable private transaction relaying,
                        withdraw-config should also be configured for ({})",
                        anchor.common.address
                    );
                }
            }
        }
        // validation checks for vanchor
        for anchor in vanchors {
            // validate config for data querying
            if config.features.data_query {
                // check if events watcher is enabled
                if !anchor.events_watcher.enabled {
                    tracing::warn!(
                        "!!WARNING!!: In order to enable data querying,
                        event-watcher should also be enabled for ({})",
                        anchor.common.address
                    );
                }
                // check if data-query is enabled in evenst-watcher config
                if !anchor.events_watcher.enable_data_query {
                    tracing::warn!(
                        "!!WARNING!!: In order to enable data querying,
                        enable-data-query in events-watcher config should also be enabled for ({})",
                        anchor.common.address
                    );
                }
            }
            // validate config for governance relaying
            if config.features.governance_relay {
                // check if proposal signing backend is configured
                if anchor.proposal_signing_backend.is_none() {
                    tracing::warn!(
                        "!!WARNING!!: In order to enable governance relaying,
                        proposal-signing-backend should be configured for ({})",
                        anchor.common.address
                    );
                }
                // check if event watchers is enabled
                if !anchor.events_watcher.enabled {
                    tracing::warn!(
                        "!!WARNING!!: In order to enable governance relaying,
                        event-watcher should also be enabled for ({})",
                        anchor.common.address
                    );
                }
                // check if linked anchor is configured
                match &anchor.linked_anchors {
                    None => {
                        tracing::warn!(
                            "!!WARNING!!: In order to enable governance relaying,
                            linked-anchors should also be configured for ({})",
                            anchor.common.address
                        );
                    }
                    Some(linked_anchors) => {
                        if linked_anchors.is_empty() {
                            tracing::warn!(
                                "!!WARNING!!: In order to enable governance relaying,
                                linked-anchors cannot be empty.
                                Please congigure Linked anchors for ({})",
                                anchor.common.address
                            );
                        } else {
                            for linked_anchor in linked_anchors {
                                let chain = linked_anchor.chain.to_lowercase();
                                let chain_defined =
                                    config.evm.contains_key(&chain);
                                if !chain_defined {
                                    tracing::warn!("!!WARNING!!: chain {} is not defined in the config.
                                        which is required by the Anchor Contract ({}) defined on {} chain.
                                        Please, define it manually, to allow the relayer to work properly.",
                                        chain,
                                        anchor.common.address,
                                        chain_name
                                    );
                                }
                            }
                        }
                    }
                }
            }
            // validate config for private transaction relaying
            if config.features.private_tx_relay {
                // check if withdraw fee is configured
                if anchor.withdraw_config.is_none() {
                    tracing::warn!(
                        "!!WARNING!!: In order to enable private transaction relaying,
                        withdraw-config should also be configured for ({})",
                        anchor.common.address
                    );
                }
            }
        }
    }
    Ok(config)
}
