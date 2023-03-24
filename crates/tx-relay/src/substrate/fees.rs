use crate::{MAX_REFUND_USD, TRANSACTION_PROFIT_USD};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde::{Deserializer, Serialize};
use std::str::FromStr;
use webb_relayer_context::RelayerContext;

const TOKEN_PRICE_USD: f64 = 0.1;
const TOKEN_DECIMALS: i32 = 18;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SubstrateFeeInfo {
    /// Estimated fee for an average relay transaction, in `wrappedToken`. Includes network fees
    /// and relay fee.
    pub estimated_fee: u128,
    /// Exchange rate for refund from `wrappedToken` to `nativeToken`
    pub refund_exchange_rate: u128,
    /// Maximum amount of `nativeToken` which can be exchanged to `wrappedToken` by relay
    pub max_refund: u128,
    /// Time when this FeeInfo was generated
    timestamp: DateTime<Utc>,
}

pub async fn get_substrate_fee_info(
    chain_id: u64,
    estimated_tx_fees: u128,
    ctx: &RelayerContext,
) -> SubstrateFeeInfo {
    let estimated_fee = estimated_tx_fees
        + matic_to_wei(TRANSACTION_PROFIT_USD / TOKEN_PRICE_USD);
    let refund_exchange_rate = matic_to_wei(1.);
    // TODO: should ensure that refund <= relayer balance
    let max_refund = matic_to_wei(MAX_REFUND_USD / TOKEN_PRICE_USD);
    SubstrateFeeInfo {
        estimated_fee,
        refund_exchange_rate,
        max_refund,
        timestamp: Utc::now(),
    }
}

/// Convert from full matic coin amount to smallest unit amount (also called wei).
///
/// It looks like subxt has no built-in functionality for this.
fn matic_to_wei(matic: f64) -> u128 {
    (matic * 10_f64.powi(TOKEN_DECIMALS)) as u128
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RpcFeeDetailsResponse {
    #[serde(deserialize_with = "deserialize_number_string")]
    pub partial_fee: u128,
}

fn deserialize_number_string<'de, D>(
    deserializer: D,
) -> std::result::Result<u128, D::Error>
where
    D: Deserializer<'de>,
{
    let hex: String = Deserialize::deserialize(deserializer)?;
    Ok(u128::from_str(&hex).unwrap())
}
