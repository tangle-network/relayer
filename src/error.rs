use webb::{evm::ethers, substrate::subxt};

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
    SubxtBasicError(#[from] subxt::BasicError),
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
}

pub type Result<T> = std::result::Result<T, Error>;
