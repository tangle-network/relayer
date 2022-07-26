use ethereum_types::U256;
use std::{collections::HashMap, sync::Arc};
use webb::evm::{
    contract::protocol_solidity::{
        fixed_deposit_anchor::{ExtData, Proof},
        FixedDepositAnchorContract,
    },
    ethers::prelude::{Signer, SignerMiddleware},
};

use crate::config::AnchorWithdrawConfig;
use crate::{
    context::RelayerContext,
    handler::{
        calculate_fee, CommandResponse, CommandStream, EvmCommand,
        NetworkStatus, WithdrawStatus,
    },
    tx_relay::evm::handle_evm_tx,
};

/// Handler for Anchor commands
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `cmd` - The command to execute
/// * `stream` - The stream to write the response to
pub async fn handle_anchor_relay_tx<'a>(
    ctx: RelayerContext,
    cmd: EvmCommand,
    stream: CommandStream,
) {
    use CommandResponse::*;
    let cmd = match cmd {
        EvmCommand::Anchor(cmd) => cmd,
        _ => return,
    };

    let requested_chain = cmd.chain_id;
    let chain = match ctx.config.evm.get(&requested_chain.to_string()) {
        Some(v) => v,
        None => {
            tracing::warn!("Unsupported Chain: {}", requested_chain);
            let _ = stream.send(Network(NetworkStatus::UnsupportedChain)).await;
            return;
        }
    };
    let supported_contracts: HashMap<_, _> = chain
        .contracts
        .iter()
        .cloned()
        .filter_map(|c| match c {
            crate::config::Contract::Anchor(c) => Some(c),
            _ => None,
        })
        .map(|c| (c.common.address, c))
        .collect();
    // get the contract configuration
    let contract_config = match supported_contracts.get(&cmd.id) {
        Some(config) => config,
        None => {
            tracing::warn!("Unsupported Contract: {:?}", cmd.id);
            let _ = stream
                .send(Network(NetworkStatus::UnsupportedContract))
                .await;
            return;
        }
    };
    // validate contract withdraw configuration
    let withdraw_config: &AnchorWithdrawConfig = match &contract_config
        .withdraw_config
    {
        Some(cfg) => cfg,
        None => {
            tracing::error!("Misconfigured Network : ({}). Please set withdraw configuration.", cmd.chain_id);
            let _ = stream
                .send(Error(format!("Misconfigured Network : ({}). Please set withdraw configuration.", cmd.chain_id)))
                .await;
            return;
        }
    };

    let wallet = match ctx.evm_wallet(&cmd.chain_id.to_string()).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("Misconfigured Network: {}", e);
            let _ = stream
                .send(Error(format!(
                    "Misconfigured Network: {:?}",
                    cmd.chain_id
                )))
                .await;
            return;
        }
    };
    // validate the relayer address first before trying
    // send the transaction.
    let reward_address = match chain.beneficiary {
        Some(account) => account,
        None => wallet.address(),
    };

    if cmd.relayer != reward_address {
        let _ = stream
            .send(Network(NetworkStatus::InvalidRelayerAddress))
            .await;
        return;
    }

    // validate that the roots are multiple of 32s
    let roots = cmd.roots.to_vec();
    if roots.len() % 32 != 0 {
        let _ = stream
            .send(Withdraw(WithdrawStatus::InvalidMerkleRoots))
            .await;
        return;
    }

    tracing::debug!(
        "Connecting to chain {:?} .. at {}",
        cmd.chain_id,
        chain.http_endpoint
    );
    let _ = stream.send(Network(NetworkStatus::Connecting)).await;
    let provider = match ctx.evm_provider(&cmd.chain_id.to_string()).await {
        Ok(value) => {
            let _ = stream.send(Network(NetworkStatus::Connected)).await;
            value
        }
        Err(e) => {
            let reason = e.to_string();
            let _ =
                stream.send(Network(NetworkStatus::Failed { reason })).await;
            let _ = stream.send(Network(NetworkStatus::Disconnected)).await;
            return;
        }
    };

    let client = SignerMiddleware::new(provider, wallet);
    let client = Arc::new(client);
    let contract = FixedDepositAnchorContract::new(cmd.id, client);
    let denomination = match contract.denomination().call().await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("Misconfigured Contract Denomination: {}", e);
            let _ = stream
                .send(Error(format!("Misconfigured Contract: {:?}", cmd.id)))
                .await;
            return;
        }
    };
    // check the fee
    let expected_fee =
        calculate_fee(withdraw_config.withdraw_fee_percentage, denomination);
    let (_, unacceptable_fee) = U256::overflowing_sub(cmd.fee, expected_fee);
    if unacceptable_fee {
        tracing::error!("Received a fee lower than configuration");
        let msg = format!(
            "User sent a fee that is too low {} but expected {}",
            cmd.fee, expected_fee,
        );
        let _ = stream.send(Error(msg)).await;
        return;
    }

    let ext_data = ExtData {
        refresh_commitment: cmd.refresh_commitment.to_fixed_bytes(),
        recipient: cmd.recipient,
        relayer: cmd.relayer,
        fee: cmd.fee,
        refund: cmd.refund,
    };

    let proof = Proof {
        roots: roots.into(),
        proof: cmd.proof,
        nullifier_hash: cmd.nullifier_hash.to_fixed_bytes(),
        ext_data_hash: cmd.ext_data_hash.to_fixed_bytes(),
    };
    tracing::trace!(?proof, ?ext_data, "Client Proof");
    let call = contract.withdraw(proof, ext_data);
    tracing::trace!("About to send Tx to {:?} Chain", cmd.chain_id);
    handle_evm_tx(call, stream).await;
}
