use crate::{MAX_REFUND_USD, TRANSACTION_PROFIT_USD};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde::{Deserializer, Serialize};
use sp_core::U256;
use std::str::FromStr;
use webb::substrate::subxt::{OnlineClient, PolkadotConfig};
use webb_relayer_context::RelayerContext;

const TOKEN_PRICE_USD: f64 = 0.1;
const TOKEN_DECIMALS: i32 = 18;

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
) -> SubstrateFeeInfo {
    let client = ctx
        .substrate_provider::<PolkadotConfig>(&chain_id.to_string())
        .await
        .unwrap();
    let properties = client.rpc().system_properties().await;
    // TODO: why is this empty???
    dbg!(properties);
    //properties.get("tokenDecimals")
    let estimated_fee = U256::from(
        estimated_tx_fees
            + native_token_to_unit(TRANSACTION_PROFIT_USD / TOKEN_PRICE_USD),
    );
    let refund_exchange_rate = native_token_to_unit(1.);
    // TODO: should ensure that refund <= relayer balance
    let max_refund = native_token_to_unit(MAX_REFUND_USD / TOKEN_PRICE_USD);
    SubstrateFeeInfo {
        estimated_fee,
        refund_exchange_rate,
        max_refund,
        timestamp: Utc::now(),
    }
}

/// Convert from full wrapped token amount to smallest unit amount.
///
/// It looks like subxt has no built-in functionality for this.
fn native_token_to_unit(matic: f64) -> U256 {
    U256::from((matic * 10_f64.powi(TOKEN_DECIMALS)) as u128)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RpcFeeDetailsResponse {
    pub partial_fee: U256,
}
