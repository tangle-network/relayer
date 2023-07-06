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

use std::sync::Arc;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use multi_provider::MultiProvider;
use webb::substrate::subxt::PolkadotConfig;
use webb::{evm::ethers, substrate::subxt};
use webb_proposals::ResourceId;

pub mod clickable_link;

/// Metrics functionality
pub mod metric;
/// Multi provider for ethers.
pub mod multi_provider;
/// A module used for debugging relayer lifecycle, sync state, or other relayer state.
pub mod probe;
/// Retry functionality
pub mod retry;
/// type-erased StaticTxPayload for Substrate Transaction queue.
pub mod static_tx_payload;

type RetryClientProvider = ethers::providers::Provider<
    ethers::providers::RetryClient<MultiProvider<ethers::providers::Http>>,
>;
/// Type alias for runtime config of Tangle client.
pub type TangleRuntimeConfig = PolkadotConfig;

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
    Axum(#[from] axum::Error),
    /// HTTP Error
    #[error(transparent)]
    Hyper(#[from] hyper::Error),
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
    EthersContractCall(
        #[from]
        ethers::contract::ContractError<
            ethers::providers::Provider<ethers::providers::Http>,
        >,
    ),
    /// Smart contract error.
    #[error(transparent)]
    EthersContractCallWithSigner(
        #[from]
        ethers::contract::ContractError<
            ethers::middleware::SignerMiddleware<
                ethers::providers::Provider<ethers::providers::Http>,
                ethers::prelude::Wallet<ethers::core::k256::ecdsa::SigningKey>,
            >,
        >,
    ),

    /// Smart contract error.
    #[error(transparent)]
    EthersContractCallWithRetry(
        #[from] ethers::contract::ContractError<RetryClientProvider>,
    ),
    /// Smart contract error.
    #[error(transparent)]
    EthersContractCallWithRetryCloneable(
        #[from] ethers::contract::ContractError<Arc<RetryClientProvider>>,
    ),
    /// Ethers Timelag provider error.
    #[error(transparent)]
    EthersTimelagRetryClientError(
        #[from] ethers::middleware::timelag::TimeLagError<RetryClientProvider>,
    ),

    /// Ethers Timelag provider error.
    #[error(transparent)]
    EthersTimelagRetryClientClonableError(
        #[from]
        ethers::middleware::timelag::TimeLagError<
            std::sync::Arc<RetryClientProvider>,
        >,
    ),

    /// Ether contract error for TimeLag provider.
    #[error(transparent)]
    EthersContractCallWithTimeLagRetryClient(
        #[from]
        ethers::contract::ContractError<
            ethers::middleware::timelag::TimeLag<RetryClientProvider>,
        >,
    ),

    /// Ether contract error for TimeLag provider.
    #[error(transparent)]
    EthersContractCallWithTimeLagRetryClientCloneable(
        #[from]
        ethers::contract::ContractError<
            ethers::middleware::timelag::TimeLag<Arc<RetryClientProvider>>,
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
    #[error(transparent)]
    GasOracle(#[from] ethers::middleware::gas_oracle::GasOracleError),
    /// Ether wallet errors.
    #[error(transparent)]
    EtherWalletError(#[from] ethers::signers::WalletError),
    /// Ethers currency conversion error
    #[error(transparent)]
    Conversion(#[from] ethers::utils::ConversionError),
    /// Failed to convert string to float
    #[error(transparent)]
    ParseFloatError(#[from] std::num::ParseFloatError),
    #[error(transparent)]
    PrometheusError(#[from] prometheus::Error),
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
    /// Restart relayer.
    #[error("Restarting Relayer")]
    RestartRelayer,
    /// Arkworks Errors.
    #[error("{}", _0)]
    ArkworksError(String),
    /// Etherscan api configuration not found.
    #[error("Etherscan api configuration not found for chain : {}", chain_id)]
    EtherscanConfigNotFound {
        /// The chain id of the node.
        chain_id: String,
    },
    #[error("No bridge registered with DKG for resource id {:?}", _0)]
    BridgeNotRegistered(ResourceId),
    #[error("Failed to fetch token price for token: {token}")]
    FetchTokenPriceError { token: String },
    #[error("Failed to read a value from substrate storage")]
    ReadSubstrateStorageError,
    #[error("Cannot convert default leaf scalar into bytes")]
    ConvertLeafScalarError,
    /// Invalid Merkle root
    #[error("Invalid Merkle root at index {}", _0)]
    InvalidMerkleRootError(u32),
    /// Missing Static Transaction Validation Details
    /// This error is raised when the static transaction validation details
    /// are missing.
    #[error("Missing Substrate Static Transaction Validation Details")]
    MissingValidationDetails,
    /// Provider not found error.
    #[error("Provider not found for index {0}")]
    ProviderNotFound(usize),
    /// Invalid Proposal bytes.
    #[error("Invalid proposal bytes")]
    InvalidProposalBytes,
    /// Invalid Proposals batch.
    #[error("Invalid proposals batch")]
    InvalidProposalsBatch,
}

/// Vanchor withdraw tx relaying errors.
#[derive(thiserror::Error, Debug)]
pub enum TransactionRelayingError {
    /// Unsupported chain
    #[error("Unsupported chain: {0}")]
    UnsupportedChain(u32),
    /// Unsupported contract address
    #[error("Unsupported contract address: {0}")]
    UnsupportedContract(String),
    /// Invalid relayer address
    #[error("Invalid relayer address: {0}")]
    InvalidRelayerAddress(String),
    /// Invalid Merkle root
    #[error("Invalid Merkle roots")]
    InvalidMerkleRoots,
    /// Invalid refund amount
    #[error("InvalidRefundAmount: {0}")]
    InvalidRefundAmount(String),
    /// Error while wrapping fee
    #[error("WrappingFeeError: {0}")]
    WrappingFeeError(String),
    /// Transaction queue error
    #[error("TransactionQueueError: {0}")]
    TransactionQueueError(String),
    /// Network Error
    #[error("Network configuration error: {0} for chain: {1}")]
    NetworkConfigurationError(String, u32),
    /// Client Error
    #[error("ClientError: {0}")]
    ClientError(String),
}

/// A type alias for the result for webb relayer, that uses the `Error` enum.
pub type Result<T> = std::result::Result<T, Error>;

impl From<Box<dyn ark_std::error::Error>> for Error {
    fn from(error: Box<dyn ark_std::error::Error>) -> Self {
        Error::ArkworksError(format!("{}", error))
    }
}

impl From<Error> for HandlerError {
    fn from(value: Error) -> Self {
        HandlerError(StatusCode::INTERNAL_SERVER_ERROR, value.to_string())
    }
}

/// Error type for HTTP handlers
pub struct HandlerError(
    /// HTTP status code for response
    pub StatusCode,
    /// Response message
    pub String,
);

impl IntoResponse for HandlerError {
    fn into_response(self) -> Response {
        (self.0, self.1).into_response()
    }
}
