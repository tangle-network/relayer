use ethereum_types::{Address, U256};
use std::sync::Arc;
use webb_relayer_context::RelayerContext;
use webb_relayer_tx_relay::evm::fees::{get_fee_info, FeeInfo};

/// Calculate fee for an average transaction over the relay. Also returns information about refund.
pub async fn calculate_fees(
    ctx: Arc<RelayerContext>,
    chain_id: u64,
    vanchor: Address,
) -> webb_relayer_utils::Result<FeeInfo> {
    // TODO: hardcoded
    let estimated_gas_amount = U256::from(1_721_713);

    get_fee_info(chain_id, vanchor, estimated_gas_amount, ctx.as_ref()).await
}
