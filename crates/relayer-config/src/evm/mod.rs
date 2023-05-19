use ethereum_types::Address;
use webb_relayer_types::{private_key::PrivateKey, rpc_url::RpcUrl};

use crate::{
    anchor::LinkedAnchorConfig, block_poller::BlockPollerConfig,
    event_watcher::EventsWatcherConfig,
    signing_backend::ProposalSigningBackendConfig,
};

use super::*;

/// EvmChainConfig is the configuration for the EVM based networks.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "kebab-case"))]
pub struct EvmChainConfig {
    /// String that groups configuration for this chain on a human-readable name.
    pub name: String,
    /// Boolean indicating EVM based networks are enabled or not.
    #[serde(default)]
    pub enabled: bool,
    /// Http(s) Endpoint for quick Req/Res
    #[serde(skip_serializing)]
    pub http_endpoint: RpcUrl,
    /// Websocket Endpoint for long living connections
    #[serde(skip_serializing)]
    pub ws_endpoint: RpcUrl,
    /// Block confirmations
    #[serde(skip_serializing, default)]
    pub block_confirmations: u32,
    /// Block Explorer for this chain.
    ///
    /// Optional, and only used for printing a clickable links
    /// for transactions and contracts.
    #[serde(skip_serializing)]
    pub explorer: Option<url::Url>,
    /// chain specific id (output of chainId opcode on EVM networks)
    pub chain_id: u32,
    /// The Private Key of this account on this network
    /// the format is more dynamic here:
    /// 1. if it starts with '0x' then this would be raw (64 bytes) hex encoded
    ///    private key.
    ///    Example: 0x8917174396171783496173419137618235192359106130478137647163400318
    ///
    /// 2. if it starts with '$' then it would be considered as an Environment variable
    ///    of a hex-encoded private key.
    ///   Example: $HARMONY_PRIVATE_KEY
    ///
    /// 3. if it starts with 'file:' then it would be considered as secrets which will
    ///    be fetched from given file path
    ///    Example: file:/Users/Bob/relayer/secrets.txt
    ///    File should include (64 bytes) hex encoded private key or valid mnemonic word list.
    ///    
    /// 4. if it starts with '> ' then it would be considered as a command that
    ///   the relayer would execute and the output of this command would be the
    ///   hex encoded private key.
    ///   Example: > pass harmony-privatekey
    ///
    /// 5. if it doesn't contains special characters and has 12 or 24 words in it
    ///   then we should process it as a mnemonic string: 'word two three four ...'
    #[serde(skip_serializing)]
    pub private_key: Option<PrivateKey>,
    /// Optionally, a user can specify an account to receive rewards for relaying
    #[serde(skip_serializing_if = "Option::is_none")]
    pub beneficiary: Option<Address>,
    /// Supported contracts over this chain.
    #[serde(default)]
    pub contracts: Vec<Contract>,
    /// TxQueue configuration
    #[serde(skip_serializing, default)]
    pub tx_queue: TxQueueConfig,
    /// Block poller/listening configuration
    #[serde(skip_serializing, default)]
    pub block_poller: Option<BlockPollerConfig>,
}

/// Linked anchor config for Evm based target system
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "kebab-case"))]
pub struct EvmLinkedAnchorConfig {
    /// The chain Id
    pub chain_id: u32,
    /// The V-anchor Contract Address.
    pub address: Address,
}

/// Enumerates the supported contract configurations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "contract")]
pub enum Contract {
    /// The VAnchor contract configuration.
    VAnchor(VAnchorContractConfig),
    /// The Open VAnchor contract configuration.
    OpenVAnchor(VAnchorContractConfig),
    /// The Signature Bridge contract configuration.
    SignatureBridge(SignatureBridgeContractConfig),
}

/// CommonContractConfig represents the common configuration for contracts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "kebab-case"))]
pub struct CommonContractConfig {
    /// The address of this contract on this chain.
    pub address: Address,
    /// the block number where this contract got deployed at.
    pub deployed_at: u64,
}

/// Smart Anchor Updates applies polices to the AnchorUpdate Proposals
/// which helps to reduce the number of updates, hence the number of
/// transactions and gas fees.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "kebab-case"))]
pub struct SmartAnchorUpdatesConfig {
    /// Enables smart anchor updates
    pub enabled: bool,
    /// Minimum time delay for the time delay sliding window
    pub min_time_delay: u64,
    /// Maximum time delay for the time delay sliding window
    pub max_time_delay: u64,
    /// Initial time delay for the time delay sliding window
    pub initial_time_delay: u64,
    /// Time delay sliding window size
    pub time_delay_window_size: usize,
}

impl Default for SmartAnchorUpdatesConfig {
    fn default() -> Self {
        Self {
            // Disabled by default
            // Experimental feature
            enabled: false,
            min_time_delay: 30,
            max_time_delay: 300,
            initial_time_delay: 10,
            time_delay_window_size: 5,
        }
    }
}

/// VAnchorContractConfig represents the configuration for the VAnchor contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "kebab-case"))]
pub struct VAnchorContractConfig {
    /// Common contract configuration.
    #[serde(flatten)]
    pub common: CommonContractConfig,
    /// Controls the events watcher
    pub events_watcher: EventsWatcherConfig,
    /// The type of the optional signing backend used for signing proposals. It can be None for pure Tx relayers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposal_signing_backend: Option<ProposalSigningBackendConfig>,
    /// A List of linked Anchor Contracts (on other chains) to this contract.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub linked_anchors: Option<Vec<LinkedAnchorConfig>>,
    /// For configuring the smart anchor updates
    #[serde(default)]
    pub smart_anchor_updates: SmartAnchorUpdatesConfig,
}

/// Signature Bridge contract configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "kebab-case"))]
pub struct SignatureBridgeContractConfig {
    /// Common contract configuration.
    #[serde(flatten)]
    pub common: CommonContractConfig,
    /// Controls the events watcher
    pub events_watcher: EventsWatcherConfig,
}
