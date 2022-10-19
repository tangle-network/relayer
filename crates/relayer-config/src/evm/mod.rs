use ethereum_types::Address;
use webb_relayer_types::{private_key::PrivateKey, rpc_url::RpcUrl};

use crate::{
    anchor::{LinkedAnchorConfig, VAnchorWithdrawConfig},
    block_poller::BlockPollerConfig,
    event_watcher::EventsWatcherConfig,
    signing_backend::ProposalSigningBackendConfig,
};

use super::*;

/// EvmChainConfig is the configuration for the EVM based networks.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
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
    /// Block Explorer for this chain.
    ///
    /// Optional, and only used for printing a clickable links
    /// for transactions and contracts.
    #[serde(skip_serializing)]
    pub explorer: Option<url::Url>,
    /// chain specific id (output of chainId opcode on EVM networks)
    #[serde(rename(serialize = "chainId"))]
    pub chain_id: u32,
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
    pub private_key: Option<PrivateKey>,
    /// Optionally, a user can specify an account to receive rewards for relaying
    pub beneficiary: Option<Address>,
    /// Supported contracts over this chain.
    #[serde(default)]
    pub contracts: Vec<Contract>,
    /// TxQueue configuration
    #[serde(skip_serializing, default)]
    pub tx_queue: TxQueueConfig,
    /// Block poller/listening configuration
    pub block_poller: Option<BlockPollerConfig>,
}

/// Linked anchor config for Evm based target system
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
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
#[serde(rename_all = "kebab-case")]
pub struct CommonContractConfig {
    /// The address of this contract on this chain.
    pub address: Address,
    /// the block number where this contract got deployed at.
    #[serde(rename(serialize = "deployedAt"))]
    pub deployed_at: u64,
}

/// VAnchorContractConfig represents the configuration for the VAnchor contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct VAnchorContractConfig {
    /// Common contract configuration.
    #[serde(flatten)]
    pub common: CommonContractConfig,
    /// Controls the events watcher
    #[serde(rename(serialize = "eventsWatcher"))]
    pub events_watcher: EventsWatcherConfig,
    /// Anchor withdraw configuration.
    #[serde(rename(serialize = "withdrawConfig"))]
    pub withdraw_config: Option<VAnchorWithdrawConfig>,
    /// The type of the optional signing backend used for signing proposals. It can be None for pure Tx relayers
    #[serde(rename(serialize = "proposalSigningBackend"))]
    pub proposal_signing_backend: Option<ProposalSigningBackendConfig>,
    /// A List of linked Anchor Contracts (on other chains) to this contract.
    #[serde(rename(serialize = "linkedAnchors"), default)]
    pub linked_anchors: Option<Vec<LinkedAnchorConfig>>,
}

/// Signature Bridge contract configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct SignatureBridgeContractConfig {
    /// Common contract configuration.
    #[serde(flatten)]
    pub common: CommonContractConfig,
    /// Controls the events watcher
    #[serde(rename(serialize = "eventsWatcher"))]
    pub events_watcher: EventsWatcherConfig,
}
