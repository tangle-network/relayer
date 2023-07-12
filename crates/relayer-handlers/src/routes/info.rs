use axum::extract::State;
use axum::Json;
use axum_client_ip::InsecureClientIp;
use std::sync::Arc;
use webb_relayer_handler_utils::IpInformationResponse;

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
        version: env!("CARGO_PKG_VERSION").into(),
        commit: env!("GIT_COMMIT").into(),
        timestamp: env!("SOURCE_TIMESTAMP").into(),
    };
    let relayer_config = RelayerConfig {
        config,
        build: build_info,
    };

    Json(RelayerInformationResponse { relayer_config })
}

/// Handles the socket address response
///
/// Returns a Result with the `IpInformationResponse` on success
///
/// # Arguments
///
/// * `ip` - Extractor for client IP, taking into account x-forwarded-for and similar headers
pub async fn handle_socket_info(
    InsecureClientIp(ip): InsecureClientIp,
) -> Json<IpInformationResponse> {
    Json(IpInformationResponse { ip: ip.to_string() })
}
