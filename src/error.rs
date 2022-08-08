use webb::{
    evm::ethers,
    substrate::{dkg_runtime, protocol_substrate_runtime, subxt},
};

/// An enum of all possible errors that could be encountered during the execution of the Webb
/// Relayer.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// An Io error occurred.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// JSON Error occurred.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    /// Config loading error.
    #[error(transparent)]
    Config(#[from] config::ConfigError),
    /// Error while iterating over a glob pattern.
    #[error(transparent)]
    GlobPattern(#[from] glob::PatternError),
    /// Error while parsing a URL.
    #[error(transparent)]
    Url(#[from] url::ParseError),
    /// Error in the underlying Http/Ws server.
    #[error(transparent)]
    Warp(#[from] warp::Error),
    /// Elliptic Curve error.
    #[error(transparent)]
    EllipticCurve(#[from] ethers::core::k256::elliptic_curve::Error),
    /// Secp256k1 error occurred.
    #[error(transparent)]
    Secp256k1(#[from] libsecp256k1::Error),
    /// Basic error for the substrate runtime.
    #[error(transparent)]
    SubxtBasicError(#[from] subxt::BasicError),
    /// DKG Runtime error.
    #[error(transparent)]
    DKGError(
        #[from]
        subxt::GenericError<
            subxt::RuntimeError<dkg_runtime::api::DispatchError>,
        >,
    ),
    /// Protocol Substrate Runtime error.
    #[error(transparent)]
    ProtocolSubstrateError(
        #[from]
        subxt::GenericError<
            subxt::RuntimeError<protocol_substrate_runtime::api::DispatchError>,
        >,
    ),
    /// Runtime metadata error.
    #[error(transparent)]
    Metadata(#[from] subxt::MetadataError),
    /// Error in Http Provider (ethers client).
    #[error(transparent)]
    EthersProvider(#[from] ethers::providers::ProviderError),
    /// Smart contract error.
    #[error(transparent)]
    EthersContract(
        #[from]
        ethers::contract::ContractError<
            ethers::providers::Provider<ethers::providers::Http>,
        >,
    ),
    /// SCALE Codec error.
    #[error(transparent)]
    ScaleCodec(#[from] webb::substrate::scale::Error),
    /// Sled database error.
    #[error(transparent)]
    Sled(#[from] sled::Error),
    /// Sled transaction error.
    #[error(transparent)]
    SledTransaction(
        #[from] sled::transaction::TransactionError<std::io::Error>,
    ),
    /// Generic error.
    #[error("{}", _0)]
    Generic(&'static str),
    /// Bridge not configured or not enabled.
    #[error("Bridge not found for: {:?}", typed_chain_id)]
    BridgeNotFound {
        /// The chain id of the bridge.
        typed_chain_id: webb_proposals::TypedChainId,
    },
    /// Error while parsing the config files.
    #[error("Config parse error: {}", _0)]
    ParseConfig(#[from] serde_path_to_error::Error<config::ConfigError>),
    /// EVM Chain not found.
    #[error("Chain Not Found: {}", chain_id)]
    ChainNotFound {
        /// The chain id of the chain.
        chain_id: String,
    },
    /// Substrate node not found.
    #[error("Node Not Found: {}", chain_id)]
    NodeNotFound {
        /// The chain id of the node.
        chain_id: String,
    },
    /// Missing Secrets in the config, either Private key, SURI, ...etc.
    #[error("Missing required private-key or SURI in the config")]
    MissingSecrets,
    /// Failed to send the response to the client.
    #[error("Failed to send response to the client")]
    FailedToSendResponse,
    /// a backgorund task failed and force restarted.
    #[error("Task Force Restarted from an error")]
    ForceRestart,
    /// a backgorund task failed and stopped upnormally.
    #[error("Task Stopped Upnormally")]
    TaskStoppedUpnormally,
}

/// A type alias for the result for webb relayer, that uses the `Error` enum.
pub type Result<T> = std::result::Result<T, Error>;
