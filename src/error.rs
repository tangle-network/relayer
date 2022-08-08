use webb::{
    evm::ethers,
    substrate::{dkg_runtime, protocol_substrate_runtime, subxt},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Config(#[from] config::ConfigError),
    #[error(transparent)]
    GlobPattern(#[from] glob::PatternError),
    #[error(transparent)]
    Url(#[from] url::ParseError),
    #[error(transparent)]
    Warp(#[from] warp::Error),
    #[error(transparent)]
    EllipticCurve(#[from] ethers::core::k256::elliptic_curve::Error),
    #[error(transparent)]
    Secp256k1(#[from] libsecp256k1::Error),
    #[error(transparent)]
    SubxtBasicError(#[from] subxt::BasicError),
    #[error(transparent)]
    DKGError(
        #[from]
        subxt::GenericError<
            subxt::RuntimeError<dkg_runtime::api::DispatchError>,
        >,
    ),
    #[error(transparent)]
    ProtocolSubstrateError(
        #[from]
        subxt::GenericError<
            subxt::RuntimeError<protocol_substrate_runtime::api::DispatchError>,
        >,
    ),
    #[error(transparent)]
    Metadata(#[from] subxt::MetadataError),
    #[error(transparent)]
    EthersProvider(#[from] ethers::providers::ProviderError),
    #[error(transparent)]
    EthersContract(
        #[from]
        ethers::contract::ContractError<
            ethers::providers::Provider<ethers::providers::Http>,
        >,
    ),
    #[error(transparent)]
    ScaleCodec(#[from] webb::substrate::scale::Error),
    #[error(transparent)]
    Sled(#[from] sled::Error),
    #[error(transparent)]
    SledTransaction(
        #[from] sled::transaction::TransactionError<std::io::Error>,
    ),
    #[error("{}", _0)]
    Generic(&'static str),
    #[error("Bridge not found for: {:?}", typed_chain_id)]
    BridgeNotFound {
        typed_chain_id: webb_proposals::TypedChainId,
    },
    #[error("Config parse error: {}", _0)]
    ParseConfig(#[from] serde_path_to_error::Error<config::ConfigError>),
    #[error("Chain Not Found: {}", chain_id)]
    ChainNotFound { chain_id: String },
    #[error("Node Not Found: {}", chain_id)]
    NodeNotFound { chain_id: String },
    #[error("Missing required private-key or SURI in the config")]
    MissingSecrets,
    #[error("Failed to send response to the client")]
    FailedToSendResponse,
    #[error("Task Force Restarted from an error")]
    ForceRestart,
    #[error("Task Stopped Upnormally")]
    TaskStoppedUpnormally,
}

pub type Result<T> = std::result::Result<T, Error>;
