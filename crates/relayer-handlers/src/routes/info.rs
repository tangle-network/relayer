use axum::extract::State;
use axum::Json;
use std::sync::Arc;

use serde::Serialize;
use webb::{
    evm::ethers::{
        prelude::k256::SecretKey,
        signers::{LocalWallet, Signer},
    },
    substrate::subxt::ext::sp_core::Pair,
};
use webb_relayer_context::RelayerContext;

/// Relayer config data
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayerInformationResponse {
    #[serde(flatten)]
    config: webb_relayer_config::WebbRelayerConfig,
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
            let key = SecretKey::from_be_bytes(key.as_bytes())?;
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
    Json(RelayerInformationResponse { config })
}
