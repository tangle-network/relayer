use crate::{
    event_watcher::EventsWatcherConfig,
    signing_backend::ProposalSigningBackendConfig,
};

use super::*;
use cosmwasm_std::Addr;
use webb_relayer_types::{
    cw_chain_id::CWChainId, mnemonic::Mnemonic, rpc_url::RpcUrl,
};

/// CosmwasmConfig is the configuration for the Cosmwasm based networks.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct CosmwasmConfig {
    /// String that groups configuration for this chain on a human-readable name.
    pub name: String,
    /// Boolean indicating Cosmwasm based networks are enabled or not.
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
    /// chain specific id (output of chainId opcode on Cosmwasm networks)
    #[serde(rename(serialize = "chainId"))]
    pub chain_id: CWChainId,
    /// The Mnemonic of this account on this network
    /// the format is more dynamic here:
    /// 1. if it starts with '$' then it would be considered as an Enviroment variable
    ///    of a hex-encoded private key.
    ///   Example: $RELAYER_MNEMONIC
    ///
    /// 2. if it starts with '> ' then it would be considered as a command that
    ///   the relayer would execute and the output of this command would be the
    ///   hex encoded private key.
    ///   Example: > pass relayer_mnemonic
    ///
    /// 3. if it doesn't contains special characters and has 12 or 24 words in it
    ///   then we should process it as a mnemonic string: 'word two three four ...'
    #[serde(skip_serializing)]
    pub mnemonic: Mnemonic,
    /// Optionally, a user can specify an account to receive rewards for relaying
    pub beneficiary: Option<Addr>,
    /// Supported contracts over this chain.
    #[serde(default)]
    pub contracts: Vec<CosmwasmContract>,
    /// TxQueue configuration
    #[serde(skip_serializing, default)]
    pub tx_queue: TxQueueConfig,
}

/// CosmwasmVAnchorWithdrawConfig is the configuration for the Cosmwasm VAnchor Withdraw.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CosmwasmVAnchorWithdrawConfig {
    /// The fee percentage that your account will receive when you relay a transaction
    /// over this chain.
    #[serde(rename(serialize = "withdrawFeePercentage"))]
    pub withdraw_fee_percentage: u8,
    /// A stringified value of the limit(Uint128) when doing a withdraw relay transaction on this chain.
    #[serde(rename(serialize = "withdrawLimit"))]
    pub withdraw_limit: String,
}

/// CosmwasmLinkedVAnchorConfig is the configuration for the cosmwasm linked Vanchor.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CosmwasmLinkedVAnchorConfig {
    /// The Chain name where this anchor belongs to.
    pub chain: String,
    /// The Anchor Contract Address.
    pub address: String,
}

/// Enumerates the supported cosmwasm-contract configurations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "contract")]
pub enum CosmwasmContract {
    /// The VAnchor contract configuration.
    VAnchor(CosmwasmVAnchorContractConfig),
    /// The Signature Bridge contract configuration.
    SignatureBridge(CosmwasmSignatureBridgeContractConfig),
}

/// CosmwasmCommonContractConfig represents the cosmwasm common configuration for contracts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CosmwasmCommonContractConfig {
    /// The address of this contract on this chain.
    pub address: String,
    /// the block number where this contract got deployed at.
    #[serde(rename(serialize = "deployedAt"))]
    pub deployed_at: u64,
}

/// CosmwasmVAnchorContractConfig represents the configuration for the Cosmwasm VAnchor contract.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CosmwasmVAnchorContractConfig {
    /// Common contract configuration.
    #[serde(flatten)]
    pub common: CosmwasmCommonContractConfig,
    /// Controls the events watcher
    #[serde(rename(serialize = "eventsWatcher"))]
    pub events_watcher: EventsWatcherConfig,
    /// Anchor withdraw configuration.
    #[serde(rename(serialize = "withdrawConfig"))]
    pub withdraw_config: Option<CosmwasmVAnchorWithdrawConfig>,
    /// The type of the optional signing backend used for signing proposals. It can be None for pure Tx relayers
    #[serde(rename(serialize = "proposalSigningBackend"))]
    pub proposal_signing_backend: Option<ProposalSigningBackendConfig>,
    /// A List of linked Anchor Contracts (on other chains) to this contract.
    #[serde(rename(serialize = "linkedAnchors"), default)]
    pub linked_anchors: Option<Vec<CosmwasmLinkedVAnchorConfig>>,
}

/// Cosmwasm Signature Bridge contract configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct CosmwasmSignatureBridgeContractConfig {
    /// Common contract configuration.
    #[serde(flatten)]
    pub common: CosmwasmCommonContractConfig,
    /// Controls the events watcher
    #[serde(rename(serialize = "eventsWatcher"))]
    pub events_watcher: EventsWatcherConfig,
}
