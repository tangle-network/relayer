use crate::substrate::balance;
use crate::{MAX_REFUND_USD, TRANSACTION_PROFIT_USD};
use chrono::{DateTime, Utc};
use serde::Serialize;
use sp_core::U256;
use std::cmp::min;
use webb::evm::ethers::utils::__serde_json::Value;
use webb::substrate::subxt::tx::PairSigner;
use webb::substrate::subxt::PolkadotConfig;
use webb_relayer_context::RelayerContext;
use webb_relayer_utils::Error;

const TOKEN_PRICE_USD: f64 = 0.1;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SubstrateFeeInfo {
    /// Estimated fee for an average relay transaction, in `wrappedToken`. Includes network fees
    /// and relay fee.
    pub estimated_fee: U256,
    /// Exchange rate for refund from `wrappedToken` to `nativeToken`
    pub refund_exchange_rate: U256,
    /// Maximum amount of `nativeToken` which can be exchanged to `wrappedToken` by relay
    pub max_refund: U256,
    /// Time when this FeeInfo was generated
    timestamp: DateTime<Utc>,
}

pub async fn get_substrate_fee_info(
    chain_id: u64,
    estimated_tx_fees: U256,
    ctx: &RelayerContext,
) -> webb_relayer_utils::Result<SubstrateFeeInfo> {
    let client = ctx
        .substrate_provider::<PolkadotConfig>(&chain_id.to_string())
        .await?;
    let decimals: i32 = client
        .rpc()
        .system_properties()
        .await?
        .get("tokenDecimals")
        .and_then(Value::as_i64)
        .ok_or(Error::ReadSubstrateStorageError)?
        as i32;
    let estimated_fee = estimated_tx_fees
        + native_token_to_unit(
            TRANSACTION_PROFIT_USD / TOKEN_PRICE_USD,
            decimals,
        );
    let refund_exchange_rate = native_token_to_unit(1., decimals);
    let max_refund =
        native_token_to_unit(MAX_REFUND_USD / TOKEN_PRICE_USD, decimals);
    let pair = ctx.substrate_wallet(&chain_id.to_string()).await?;
    let signer = PairSigner::new(pair.clone());

    let max_refund =
        min(max_refund, U256::from(balance(client, signer).await?));
    Ok(SubstrateFeeInfo {
        estimated_fee,
        refund_exchange_rate,
        max_refund,
        timestamp: Utc::now(),
    })
}

/// Convert from full wrapped token amount to smallest unit amount.
///
/// It looks like subxt has no built-in functionality for this.
fn native_token_to_unit(matic: f64, token_decimals: i32) -> U256 {
    U256::from((matic * 10_f64.powi(token_decimals)) as u128)
}
