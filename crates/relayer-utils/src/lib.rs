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

use webb::{evm::ethers, substrate::subxt};

pub mod clickable_link;

/// Metrics functionality
pub mod metric;
/// A module used for debugging relayer lifecycle, sync state, or other relayer state.
pub mod probe;
/// Retry functionality
pub mod retry;

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
    /// Error from Glob Iterator.
    #[error(transparent)]
    Glob(#[from] glob::GlobError),
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
    SubxtError(#[from] subxt::error::Error),
    /// Runtime metadata error.
    #[error(transparent)]
    Metadata(#[from] subxt::error::MetadataError),
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
    /// Smart contract error.
    #[error(transparent)]
    EthersContract2(
        #[from]
        ethers::contract::ContractError<
            ethers::middleware::SignerMiddleware<
                ethers::providers::Provider<ethers::providers::Http>,
                ethers::prelude::Wallet<ethers::core::k256::ecdsa::SigningKey>,
            >,
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
    /// Reqwest error
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    /// Etherscan API error
    #[error(transparent)]
    Etherscan(#[from] ethers::etherscan::errors::EtherscanError),
    /// Ethers currency conversion error
    #[error(transparent)]
    Conversion(#[from] ethers::utils::ConversionError),
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
    /// a backgorund task failed and stopped Abnormally.
    #[error("Task Stopped Apnormally")]
    TaskStoppedAbnormally,
}

/// A type alias for the result for webb relayer, that uses the `Error` enum.
pub type Result<T> = std::result::Result<T, Error>;
