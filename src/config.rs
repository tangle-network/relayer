use std::collections::HashMap;
use std::path::Path;
use std::str::FromStr;

use config::FileFormat;
use serde::{Deserialize, Serialize};
use webb::evm::ethereum_types::{Address, Secret, U256};

const fn default_port() -> u16 {
    9955
}

const fn enable_leaves_watcher_default() -> bool {
    true
}

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
    pub evm: HashMap<String, ChainConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ChainConfig {
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
    pub contracts: Vec<Contract>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct EventsWatcherConfig {
    #[serde(default = "enable_leaves_watcher_default")]
    /// if it is enabled for this chain or not.
    pub enabled: bool,
    /// Polling interval in milliseconds
    #[serde(rename(serialize = "pollingInterval"))]
    pub polling_interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct AnchorWithdrawConfig {
    /// The fee percentage that your account will receive when you relay a transaction
    /// over this chain.
    #[serde(rename(serialize = "withdrawFeePercentage"))]
    pub withdraw_fee_percentage: f64,
    /// A hex value of the gaslimit when doing a withdraw relay transaction on this chain.
    #[serde(skip_serializing)]
    pub withdraw_gaslimit: U256,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct LinkedAnchorConfig {
    /// The Chain name where this anchor belongs to.
    /// and it is case-insensitive.
    pub chain: String,
    /// The Anchor Contract Address.
    pub address: Address,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "contract")]
pub enum Contract {
    Tornado(TornadoContractConfig),
    Anchor(AnchorContractConfig),
    Bridge(BridgeContractConfig),
    GovernanceBravoDelegate(GovernanceBravoDelegateContractConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CommonContractConfig {
    /// The address of this contract on this chain.
    pub address: Address,
    /// the block number where this contract got deployed at.
    #[serde(rename(serialize = "deployedAt"))]
    pub deployed_at: u64,
}

#[derive(Debug, Clone)]
pub struct SerializeableAddress(Address);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct TornadoContractConfig {
    #[serde(flatten)]
    pub common: CommonContractConfig,
    /// Controls the events watcher
    #[serde(rename(serialize = "eventsWatcher"))]
    pub events_watcher: EventsWatcherConfig,
    /// The size of this contract
    pub size: f64,
    /// Anchor withdraw configuration.
    #[serde(flatten)]
    pub withdraw_config: AnchorWithdrawConfig,
}

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
    #[serde(flatten)]
    pub withdraw_config: AnchorWithdrawConfig,
    /// A List of linked Anchor Contracts (on other chains) to this contract.
    #[serde(rename(serialize = "linkedAnchors"))]
    pub linked_anchors: Vec<LinkedAnchorConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct BridgeContractConfig {
    #[serde(flatten)]
    pub common: CommonContractConfig,
    /// Controls the events watcher
    #[serde(rename(serialize = "eventsWatcher"))]
    pub events_watcher: EventsWatcherConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct GovernanceBravoDelegateContractConfig {
    #[serde(flatten)]
    pub common: CommonContractConfig,
    // TODO(@shekohex): add more fields here...
}

#[derive(Debug, Clone)]
pub struct PrivateKey(Secret);

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
                        Err(e) => Err(serde::de::Error::custom(format!("{}\n got {} but expected a 66 string (including the 0x prefix)", e, value))),
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
                        Err(e) => Err(serde::de::Error::custom(format!("{}\n got {} but expected a 66 chars string (including the 0x prefix) but found {} char", e, val, val.len()))),
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

pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<WebbRelayerConfig> {
    let mut cfg = config::Config::new();
    // A pattern that covers all toml files in the config directory and subdirectories.
    let pattern = format!("{}/**/*.toml", path.as_ref().display());
    // then get an iterator over all matching files
    let config_files = glob::glob(&pattern)?.flatten();
    for config_file in config_files {
        let base = config_file.display().to_string();
        let file = config::File::from(config_file).format(FileFormat::Toml);
        let mut incremental_config = config::Config::new();
        incremental_config.merge(file.clone())?;
        let config: Result<
            WebbRelayerConfig,
            serde_path_to_error::Error<config::ConfigError>,
        > = serde_path_to_error::deserialize(incremental_config);
        match config {
            Ok(_) => {
                // merge the file into the cfg
                cfg.merge(file)?;
            }
            Err(e) => {
                // print the issue that occurred while deserializing, then skip that config
                tracing::debug!(
                    "Error {} while attempting to deserialize {}",
                    e,
                    base
                );
            }
        };
    }
    // also merge in the environment (with a prefix of WEBB).
    cfg.merge(config::Environment::with_prefix("WEBB").separator("_"))?;
    // and finally deserialize the config and post-process it
    let config = serde_path_to_error::deserialize(cfg);
    match config {
        Ok(config) => dbg!(postloading_process(config)),
        Err(e) => {
            tracing::error!("{}", e);
            anyhow::bail!("Error while loading config files")
        }
    }
}

fn postloading_process(
    mut config: WebbRelayerConfig,
) -> anyhow::Result<WebbRelayerConfig> {
    tracing::trace!("Checking configration sanity ...");
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
    // check that all required chains are already present in the config.
    for (chain_name, chain_config) in &config.evm {
        let anchors = chain_config.contracts.iter().filter_map(|c| match c {
            Contract::Anchor(cfg) => Some(cfg),
            _ => None,
        });
        for anchor in anchors {
            for linked_anchor in &anchor.linked_anchors {
                let chain = linked_anchor.chain.to_lowercase();
                let chain_defined = config.evm.contains_key(&chain);
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
    Ok(config)
}
