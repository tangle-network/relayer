use super::*;
use webb_relayer_types::{rpc_url::RpcUrl, suri::Suri};

use crate::event_watcher::EventsWatcherConfig;

/// SubstrateConfig is the relayer configuration for the Substrate based networks.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "kebab-case"))]
pub struct SubstrateConfig {
    /// String that groups configuration for this chain on a human-readable name.
    pub name: String,
    /// Boolean indicating Substrate networks are enabled or not.
    #[serde(default)]
    pub enabled: bool,
    /// Http(s) Endpoint for quick Req/Res
    #[serde(skip_serializing)]
    pub http_endpoint: RpcUrl,
    /// Websocket Endpoint for long living connections
    #[serde(skip_serializing)]
    pub ws_endpoint: RpcUrl,
    /// Block Explorer for this Substrate node.
    ///
    /// Optional, and only used for printing a clickable links
    /// for transactions and contracts.
    #[serde(skip_serializing)]
    pub explorer: Option<url::Url>,
    /// chain specific id (output of ChainIdentifier constant on LinkableTree Pallet)
    pub chain_id: u32,
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
    pub suri: Option<Suri>,
    /// Supported pallets over this substrate node.
    #[serde(default)]
    pub pallets: Vec<Pallet>,
    /// TxQueue configuration
    #[serde(skip_serializing, default)]
    pub tx_queue: TxQueueConfig,
}

/// Enumerates the supported pallets configurations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "pallet")]
pub enum Pallet {
    /// Jobs pallet
    Jobs(JobsPalletConfig),
}

/// JobsPalletConfig represents the configuration for the Jobs pallet.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "kebab-case"))]
pub struct JobsPalletConfig {
    /// Controls the events watcher
    pub events_watcher: EventsWatcherConfig,
}
