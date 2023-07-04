use std::sync::Arc;

use axum::extract::{Path, State};

use axum::Json;
use ethereum_types::{Address, H512};
use serde::Serialize;
use webb_relayer_context::RelayerContext;
use webb_relayer_handler_utils::{EvmVanchorCommand, SubstrateVAchorCommand};
use webb_relayer_tx_relay::evm::vanchor::handle_vanchor_relay_tx;
use webb_relayer_tx_relay::substrate::vanchor::handle_substrate_vanchor_relay_tx;
use webb_relayer_utils::HandlerError;

/// Success response for withdrawal tx API request.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawTxSuccessResponse {
    status: String,
    message: String,
    item_key: H512,
}

/// Failure response for withdrawal tx API request.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawTxFailureResponse {
    status: String,
    message: String,
    reason: String,
}

/// Withdrawal transaction API request response
#[derive(Debug, Serialize)]
#[serde(untagged)]
#[serde(rename_all = "camelCase")]
pub enum WithdrawTxResponse {
    /// Success response for withdrawal tx API request.
    Success(WithdrawTxSuccessResponse),
    /// Failure response for withdrawal tx API request.
    Failure(WithdrawTxFailureResponse),
}

/// Handles private tx withdraw request for evm chains.
///
/// Returns a Result with the `WithdrawTxResponse`.
///
/// # Arguments
///
/// * `chain_id` - An u32 representing the chain id of the chain.
/// * `contract` - An address of the contract to submit transaction.
pub async fn handle_private_tx_withdraw_evm(
    State(ctx): State<Arc<RelayerContext>>,
    Path((chain_id, contract)): Path<(u32, Address)>,
    Json(payload): Json<EvmVanchorCommand>,
) -> Result<Json<WithdrawTxResponse>, HandlerError> {
    tracing::debug!("Received withdrawal request for chain : {} vanchor contract : {} with payload: {:?} ",
        chain_id, contract, payload
    );
    let response = handle_vanchor_relay_tx(ctx, payload).await;

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
                reason: format!("{}", e),
            };

            Ok(Json(WithdrawTxResponse::Failure(response)))
        }
    }
}

/// Handles private tx withdraw request for substrate chains.
///
/// Returns a Result with the `WithdrawTxResponse`.
///
/// # Arguments
///
/// * `chain_id` - An u32 representing the chain id of the chain.
/// * `tree_id` - An u32 representing vanchor tree id.
pub async fn handle_private_tx_withdraw_substrate(
    State(ctx): State<Arc<RelayerContext>>,
    Path((chain_id, tree_id)): Path<(u32, u32)>,
    Json(payload): Json<SubstrateVAchorCommand>,
) -> Result<Json<WithdrawTxResponse>, HandlerError> {
    tracing::debug!("Received withdrawal request for chain : {} vanchor tree : {} with payload: {:?} ",
        chain_id, tree_id, payload
    );
    let response = handle_substrate_vanchor_relay_tx(ctx, payload).await;

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
                reason: format!("{}", e),
            };

            Ok(Json(WithdrawTxResponse::Failure(response)))
        }
    }
}
