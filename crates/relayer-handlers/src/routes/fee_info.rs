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
