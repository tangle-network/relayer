#![allow(clippy::large_enum_variant)]
#![warn(missing_docs)]
use axum::extract::{Path, State};
use axum::Json;
use ethereum_types::{Address, U256};
use std::sync::Arc;
use webb_proposals::TypedChainId;
use webb_relayer_context::RelayerContext;
use webb_relayer_tx_relay::evm::fees::{get_evm_fee_info, EvmFeeInfo};
use webb_relayer_utils::HandlerError;

/// Handler for fee estimation
///
/// # Arguments
///
/// * `chain_id` - ID of the blockchain
/// * `vanchor` - Address of the smart contract
/// * `gas_amount` - How much gas the transaction needs. Don't use U256 here because it
///                  gets parsed incorrectly.
pub async fn handle_evm_fee_info(
    State(ctx): State<Arc<RelayerContext>>,
    Path((chain_id, vanchor, gas_amount)): Path<(u32, Address, u64)>,
) -> Result<Json<EvmFeeInfo>, HandlerError> {
    let chain_id = TypedChainId::Evm(chain_id);
    let gas_amount = U256::from(gas_amount);
    Ok(
        get_evm_fee_info(chain_id, vanchor, gas_amount, ctx.as_ref())
            .await
            .map(Json)?,
    )
}
