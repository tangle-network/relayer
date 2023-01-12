use serde::Serialize;
use std::sync::Arc;
use webb_relayer_context::RelayerContext;
use webb_relayer_utils::fees::{calculate_exchange_rate, estimate_gas_price};

/// Return value of fee_info API call. Contains information about relay transaction fee and refunds.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeInfo {
    /// Estimated fee for an average relay transaction, in `wrappedToken`
    /// TODO: We dont know the actual gas amount of the transaction here, so it would make more
    ///       sense to return a gas price (or multiple), and then calculate the fee on the client.
    fee: f32,
    /// Exchange rate for refund from `wrappedToken` to `nativeToken`
    refund_exchange_rate: f32,
    /// Maximum amount of `wrappedToken` which can be exchanged to `nativeToken` by relay
    max_refund: f32,
}

/// Calculate fee for an average transaction over the relay. Also returns information about refund.
pub async fn calculate_fees(
    chain_id: u32,
    ctx: Arc<RelayerContext>,
) -> webb_relayer_utils::Result<FeeInfo> {
    let chain = ctx.config.evm.get(&chain_id.to_string()).unwrap();

    // TODO: these are all hardcoded for now
    let gas_amount = 1_721_713;
    let max_refund_native = 1.;

    let gas_price = estimate_gas_price().await.unwrap();

    let exchange_rate =
        calculate_exchange_rate("usd-coin", "ethereum").await as f32;

    let native_fee = (gas_price * gas_amount) as f32;
    let wrapped_fee = native_fee * exchange_rate;
    let max_refund_wrapped = max_refund_native * exchange_rate;
    Ok(FeeInfo {
        fee: wrapped_fee,
        refund_exchange_rate: exchange_rate,
        max_refund: max_refund_wrapped,
    })
}
