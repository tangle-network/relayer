use std::{convert::Infallible, sync::Arc};

use serde::Serialize;
use webb::{
    evm::ethers::{
        prelude::k256::SecretKey,
        signers::{LocalWallet, Signer},
    },
    substrate::subxt::ext::sp_core::Pair,
};
use webb_relayer_context::RelayerContext;

/// Handles relayer configuration requests
///
/// Returns a Result with the `RelayerConfigurationResponse` on success
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
pub async fn handle_relayer_info(
    ctx: Arc<RelayerContext>,
) -> Result<impl warp::Reply, Infallible> {
    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct RelayerInformationResponse {
        #[serde(flatten)]
        config: webb_relayer_config::WebbRelayerConfig,
    }
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
    Ok(warp::reply::json(&RelayerInformationResponse { config }))
}
