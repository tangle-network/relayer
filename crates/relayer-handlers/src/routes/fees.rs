use ethereum_types::U256;
use webb_relayer_utils::fees::{get_fee_info, FeeInfo};

/// Calculate fee for an average transaction over the relay. Also returns information about refund.
pub async fn calculate_fees() -> webb_relayer_utils::Result<FeeInfo> {
    // TODO: hardcoded
    let estimated_gas_amount = U256::from(1_721_713);
    // TODO: need to get the actual tokens which are being exchanged (also in handle_vanchor_relay_tx)
    let wrapped_token = "usd-coin";
    let base_token = "ethereum";

    get_fee_info(estimated_gas_amount, wrapped_token, base_token).await
}
