use super::*;
use axum::extract::{Path, State};
use std::sync::Arc;

use axum::Json;
use ethereum_types::Address;
use webb_proposals::TypedChainId;
use webb_relayer_context::RelayerContext;
use webb_relayer_handler_utils::EvmVanchorCommand;
use webb_relayer_tx_relay::evm::vanchor::handle_vanchor_relay_tx;
use webb_relayer_utils::HandlerError;

/// Handles private tx withdraw request for evm chains.
///
/// Returns a Result with the `WithdrawTxResponse`.
///
/// # Arguments
///
/// * `chain_id` - An u32 representing the chain id of the chain.
/// * `contract` - An address of the contract to submit transaction.
/// * `payload` - An EvmVanchorCommand struct containing the command to execute.
pub async fn handle_private_tx_withdraw_evm(
    State(ctx): State<Arc<RelayerContext>>,
    Path((chain_id, contract)): Path<(u32, Address)>,
    Json(payload): Json<EvmVanchorCommand>,
) -> Result<Json<WithdrawTxResponse>, HandlerError> {
    tracing::debug!(%chain_id, %contract, ?payload, "Received withdrawal request");
    let response = handle_vanchor_relay_tx(
        ctx,
        TypedChainId::Evm(chain_id),
        contract,
        payload,
    )
    .await;

    match response {
        Ok(tx_item_key) => {
            let response = WithdrawTxSuccessResponse {
                status: "Sent".to_string(),
                message: "Transaction sent successfully".to_string(),
                item_key: tx_item_key,
            };
            Ok(Json(WithdrawTxResponse::Success(response)))
        }
        Err(e) => {
            let response = WithdrawTxFailureResponse {
                status: "Failed".to_string(),
                message: "Transaction request failed".to_string(),
                reason: format!("{e}"),
            };
            Ok(Json(WithdrawTxResponse::Failure(response)))
        }
    }
}
