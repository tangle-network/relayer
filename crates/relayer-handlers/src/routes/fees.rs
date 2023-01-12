use ethereum_types::U256;
use serde::Serialize;
use webb_relayer_utils::fees::max_refund;
use webb_relayer_utils::fees::{
    calculate_exchange_rate, calculate_wrapped_fee,
};

/// Return value of fee_info API call. Contains information about relay transaction fee and refunds.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeInfo {
    /// Estimated fee for an average relay transaction, in `wrappedToken`
    /// TODO: We dont know the actual gas amount of the transaction here, so it would make more
    ///       sense to return a gas price, and then calculate the fee on the client using estimated
    ///       gas amount. Then we could remove the hardcoded `estimated_gas_amount`.
    fee: U256,
    /// Exchange rate for refund from `wrappedToken` to `nativeToken`
    refund_exchange_rate: f64,
    /// Maximum amount of `wrappedToken` which can be exchanged to `nativeToken` by relay
    max_refund: U256,
}

/// Calculate fee for an average transaction over the relay. Also returns information about refund.
pub async fn calculate_fees() -> webb_relayer_utils::Result<FeeInfo> {
    // TODO: hardcoded
    let estimated_gas_amount = U256::from(1_721_713);

    // TODO: need to get the actual tokens which are being exchanged (also in handle_vanchor_relay_tx)
    let exchange_rate = calculate_exchange_rate("usd-coin", "ethereum").await;

    let wrapped_fee =
        calculate_wrapped_fee(estimated_gas_amount, exchange_rate).await;
    let max_refund = max_refund("usd-coin").await;

    Ok(FeeInfo {
        fee: wrapped_fee,
        refund_exchange_rate: exchange_rate,
        max_refund,
    })
}
