use axum::extract::State;
use axum::Json;
use std::sync::Arc;

use serde::Serialize;
use sp_core::Pair;
use webb::evm::ethers::{
    prelude::k256::SecretKey,
    signers::{LocalWallet, Signer},
};
use webb_relayer_context::RelayerContext;

/// Build info data
#[derive(Debug, Serialize)]
pub struct BuildInfo {
    /// Version of the relayer
    pub version: String,
    /// Commit hash of the relayer
    pub commit: String,
    /// Build time of the relayer
    pub timestamp: String,
}

/// Relayer config data
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayerConfig {
    /// Relayer chain config
    #[serde(flatten)]
    pub config: webb_relayer_config::WebbRelayerConfig,
    /// Relayer build info
    pub build: BuildInfo,
}

/// Relayer configuration response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayerInformationResponse {
    #[serde(flatten)]
    relayer_config: RelayerConfig,
}

/// Handles relayer configuration requests
///
/// Returns a Result with the `RelayerConfigurationResponse` on success
pub async fn handle_relayer_info(
    State(ctx): State<Arc<RelayerContext>>,
) -> Json<RelayerInformationResponse> {
    // clone the original config, to update it with accounts.
    let mut config = ctx.config.clone();

    let _ = config
        .evm
        .values_mut()
        .filter(|v| v.beneficiary.is_none())
        .try_for_each(|v| {
            let key = v
                .private_key
                .as_ref()
                .ok_or(webb_relayer_utils::Error::MissingSecrets)?;
            let key = SecretKey::from_bytes(key.as_bytes().into())?;
            let wallet = LocalWallet::from(key);
            v.beneficiary = Some(wallet.address());
            webb_relayer_utils::Result::Ok(())
        });
    let _ = config
        .substrate
        .values_mut()
        .filter(|v| v.beneficiary.is_none())
        .try_for_each(|v| {
            let suri = v
                .suri
                .as_ref()
                .ok_or(webb_relayer_utils::Error::MissingSecrets)?;
            v.beneficiary = Some(suri.public());
            webb_relayer_utils::Result::Ok(())
        });

    // Build info
    let build_info = BuildInfo {
        version: std::env::var("CARGO_PKG_VERSION").unwrap_or_default(),
        commit: std::env::var("GIT_COMMIT").unwrap_or_default(),
        timestamp: std::env::var("SOURCE_TIMESTAMP").unwrap_or_default(),
    };
    let relayer_config = RelayerConfig {
        config,
        build: build_info,
    };

    Json(RelayerInformationResponse { relayer_config })
}
