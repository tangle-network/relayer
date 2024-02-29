// Copyright (C) 2022-2024 Webb Technologies Inc.
//
// Tangle is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Tangle is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should receive a copy of the GNU General Public License
// If not, see <http://www.gnu.org/licenses/>.

use super::*;
use axum::extract::{Path, State};
use std::sync::Arc;

use axum::Json;
use ethereum_types::Address;
use webb_proposals::TypedChainId;
use webb_relayer_context::RelayerContext;
use webb_relayer_handler_utils::EvmVanchorCommand;
use webb_relayer_tx_relay::evm::masp_vanchor::handle_masp_vanchor_relay_tx;
use webb_relayer_utils::HandlerError;

/// Handles MASP tx withdrawal relaying request for evm chains.
///
/// Returns a Result with the `WithdrawTxResponse`.
///
/// # Arguments
///
/// * `chain_id` - An u32 representing the chain id of the chain.
/// * `contract` - An address of the contract to submit transaction.
/// * `payload` - An EvmVanchorCommand struct containing the command to execute.
pub async fn handle_masp_tx_relaying_evm(
    State(ctx): State<Arc<RelayerContext>>,
    Path((chain_id, contract)): Path<(u32, Address)>,
    Json(payload): Json<EvmVanchorCommand>,
) -> Result<Json<WithdrawTxResponse>, HandlerError> {
    tracing::debug!(%chain_id, %contract, ?payload, "Received withdrawal request");
    let response = handle_masp_vanchor_relay_tx(
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
