use super::*;
use crate::evm::fees::{get_evm_fee_info, EvmFeeInfo};
use crate::evm::handle_evm_tx;
use ethereum_types::U256;
use std::{collections::HashMap, sync::Arc};
use webb::evm::ethers::utils::{format_units, parse_ether};
use webb::evm::{
    contract::protocol_solidity::{
        variable_anchor::{CommonExtData, Encryptions, PublicInputs},
        VAnchorContract,
    },
    ethers::prelude::{Signer, SignerMiddleware},
};
use webb_proposals::{ResourceId, TargetSystem, TypedChainId};
use webb_relayer_context::RelayerContext;
use webb_relayer_handler_utils::EvmVanchorCommand;
use webb_relayer_handler_utils::{CommandStream, NetworkStatus};

/// Handler for VAnchor commands
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `cmd` - The command to execute
/// * `stream` - The stream to write the response to
pub async fn handle_vanchor_relay_tx<'a>(
    ctx: RelayerContext,
    cmd: EvmVanchorCommand,
    stream: CommandStream,
) -> Result<(), CommandResponse> {
    use CommandResponse::*;

    let requested_chain = cmd.chain_id;
    let chain = ctx
        .config
        .evm
        .get(&requested_chain.to_string())
        .ok_or(Network(NetworkStatus::UnsupportedChain))?;
    let supported_contracts: HashMap<_, _> = chain
        .contracts
        .iter()
        .cloned()
        .filter_map(|c| match c {
            webb_relayer_config::evm::Contract::VAnchor(c) => Some(c),
            _ => None,
        })
        .map(|c| (c.common.address, c))
        .collect();
    // get the contract configuration
    let contract_config = supported_contracts
        .get(&cmd.id)
        .ok_or(Network(NetworkStatus::UnsupportedContract))?;

    let wallet =
        ctx.evm_wallet(&cmd.chain_id.to_string())
            .await
            .map_err(|e| {
                Error(format!("Misconfigured Network: {:?}, {e}", cmd.chain_id))
            })?;
    // validate the relayer address first before trying
    // send the transaction.
    let reward_address = chain.beneficiary.unwrap_or(wallet.address());

    if cmd.ext_data.relayer != reward_address {
        return Err(Network(NetworkStatus::InvalidRelayerAddress));
    }

    // validate that the roots are multiple of 32s
    let roots = cmd.proof_data.roots.to_vec();
    if roots.len() % 32 != 0 {
        return Err(Withdraw(WithdrawStatus::InvalidMerkleRoots));
    }

    tracing::debug!(
        "Connecting to chain {:?} .. at {}",
        cmd.chain_id,
        chain.http_endpoint
    );
    let _ = stream.send(Network(NetworkStatus::Connecting)).await;
    let provider =
        ctx.evm_provider(&cmd.chain_id.to_string())
            .await
            .map_err(|e| {
                Network(NetworkStatus::Failed {
                    reason: e.to_string(),
                })
            })?;
    let _ = stream.send(Network(NetworkStatus::Connected)).await;

    let client = Arc::new(SignerMiddleware::new(provider, wallet));
    let contract = VAnchorContract::new(cmd.id, client.clone());

    let common_ext_data = CommonExtData {
        recipient: cmd.ext_data.recipient,
        ext_amount: cmd.ext_data.ext_amount.0,
        relayer: cmd.ext_data.relayer,
        fee: cmd.ext_data.fee,
        refund: cmd.ext_data.refund,
        token: cmd.ext_data.token,
    };
    let public_inputs = PublicInputs {
        roots: roots.into(),
        extension_roots: cmd.proof_data.extension_roots,
        input_nullifiers: cmd
            .proof_data
            .input_nullifiers
            .iter()
            .map(|v| v.to_fixed_bytes().into())
            .collect(),
        output_commitments: cmd
            .proof_data
            .output_commitments
            .into_iter()
            .map(|c| U256::from(c.to_fixed_bytes()))
            .collect::<Vec<_>>()
            .try_into()
            .unwrap_or_default(),
        public_amount: U256::from_big_endian(
            &cmd.proof_data.public_amount.to_fixed_bytes(),
        ),
        ext_data_hash: cmd.proof_data.ext_data_hash.to_fixed_bytes().into(),
    };

    let encryptions = Encryptions {
        encrypted_output_1: cmd.ext_data.encrypted_output1,
        encrypted_output_2: cmd.ext_data.encrypted_output2,
    };

    tracing::trace!(?cmd.proof_data.proof, ?common_ext_data, "Client Proof");

    let mut call = contract.transact(
        cmd.proof_data.proof,
        [0u8; 32].into(),
        common_ext_data,
        public_inputs,
        encryptions,
    );
    if !cmd.ext_data.refund.is_zero() {
        call = call.value(cmd.ext_data.refund);
    }

    let gas_amount =
        client.estimate_gas(&call.tx, None).await.map_err(|e| {
            Network(NetworkStatus::Failed {
                reason: e.to_string(),
            })
        })?;
    let typed_chain_id = TypedChainId::Evm(chain.chain_id);
    let fee_info = get_evm_fee_info(
        typed_chain_id,
        contract_config.common.address,
        gas_amount,
        &ctx,
    )
    .await
    .map_err(|e| {
        Network(NetworkStatus::Failed {
            reason: e.to_string(),
        })
    })?;

    // validate refund amount
    if cmd.ext_data.refund > fee_info.max_refund {
        let msg = format!(
            "User requested a refund which is higher than the maximum of {}",
            fee_info.max_refund
        );
        return Err(Error(msg));
    }

    // check the fee
    // TODO: This adjustment could potentially be exploited
    let adjusted_fee = fee_info.estimated_fee / 100 * 96;
    let wrapped_amount =
        calculate_wrapped_refund_amount(cmd.ext_data.refund, &fee_info)
            .map_err(|e| {
                Error(format!("Failed to calculate wrapped refund amount: {e}"))
            })?;
    if cmd.ext_data.fee < adjusted_fee + wrapped_amount {
        let msg = format!(
            "User sent a fee that is too low {} but expected {}",
            cmd.ext_data.fee, adjusted_fee + wrapped_amount
        );
        return Err(Error(msg));
    }

    let target_system = TargetSystem::new_contract_address(
        contract_config.common.address.to_fixed_bytes(),
    );
    let resource_id = ResourceId::new(target_system, typed_chain_id);

    tracing::trace!("About to send Tx to {:?} Chain", cmd.chain_id);
    handle_evm_tx(call, stream, cmd.chain_id, ctx.metrics.clone(), resource_id)
        .await?;

    // update metric
    let metrics_clone = ctx.metrics.clone();
    let mut metrics = metrics_clone.lock().await;
    // update metric for total fee earned by relayer on particular resource
    metrics
        .resource_metric_entry(resource_id)
        .total_fee_earned
        .inc_by(cmd.ext_data.fee.as_u128() as f64);

    // update metric for total fee earned by relayer
    metrics
        .total_fee_earned
        .inc_by(cmd.ext_data.fee.as_u128() as f64);

    metrics
        .account_balance_entry(typed_chain_id)
        .set(wei_to_gwei(relayer_balance));
    Ok(())
}

fn calculate_wrapped_refund_amount(
    refund: U256,
    fee_info: &EvmFeeInfo,
) -> webb_relayer_utils::Result<U256> {
    let refund_exchange_rate: f32 =
        format_units(fee_info.refund_exchange_rate, "ether")?.parse()?;
    let refund_amount: f32 = format_units(refund, "ether")?.parse()?;
    Ok(parse_ether(refund_amount / refund_exchange_rate)?)
}
