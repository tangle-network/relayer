use ethereum_types::U256;
use scale::Encode;
use std::{collections::HashMap, sync::Arc, time::Duration};
use webb::evm::{
    contract::protocol_solidity::{
        fixed_deposit_anchor::{ExtData, Proof},
        FixedDepositAnchorContract,
    },
    ethers::{
        prelude::{Signer, SignerMiddleware},
        utils::keccak256,
    },
};

use crate::{
    context::RelayerContext,
    handler::{
        calculate_fee, into_withdraw_error, CommandResponse, CommandStream,
        EvmCommand, NetworkStatus, WithdrawStatus,
    },
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
        EvmCommand::AnchorRelayTx(cmd) => cmd,
        _ => return,
    };

    let requested_chain = cmd.chain.to_lowercase();
    let chain = match ctx.config.evm.get(&requested_chain) {
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

    let wallet = match ctx.evm_wallet(&cmd.chain).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("Misconfigured Network: {}", e);
            let _ = stream
                .send(Error(format!("Misconfigured Network: {:?}", cmd.chain)))
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
        cmd.chain,
        chain.http_endpoint
    );
    let _ = stream.send(Network(NetworkStatus::Connecting)).await;
    let provider = match ctx.evm_provider(&cmd.chain).await {
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
    let expected_fee = calculate_fee(
        contract_config.withdraw_config.withdraw_fee_percentage,
        denomination,
    );
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

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&ext_data.refresh_commitment);
    bytes.extend_from_slice(ext_data.recipient.as_bytes());
    bytes.extend_from_slice(ext_data.relayer.as_bytes());
    bytes.extend_from_slice(&ext_data.fee.encode());
    bytes.extend_from_slice(&ext_data.refund.encode());

    let ext_data_hash = keccak256(bytes);
    let proof = Proof {
        roots: roots.into(),
        proof: cmd.proof,
        nullifier_hash: cmd.nullifier_hash.to_fixed_bytes(),
        ext_data_hash: ext_data_hash,
    };
    tracing::trace!(?proof, ?ext_data, "Client Proof");
    let call = contract.withdraw(proof, ext_data);
    // Make a dry call, to make sure the transaction will go through successfully
    // to avoid wasting fees on invalid calls.
    match call.call().await {
        Ok(_) => {
            let _ = stream.send(Withdraw(WithdrawStatus::Valid)).await;
            tracing::debug!("Proof is valid");
        }
        Err(e) => {
            tracing::error!("Error Client sent an invalid proof: {}", e);
            let err = into_withdraw_error(e);
            let _ = stream.send(Withdraw(err)).await;
            return;
        }
    };
    tracing::trace!("About to send Tx to {:?} Chain", cmd.chain);
    let tx = match call.send().await {
        Ok(pending) => {
            let _ = stream.send(Withdraw(WithdrawStatus::Sent)).await;
            let tx_hash = *pending;
            tracing::debug!(%tx_hash, "Tx is submitted and pending!");
            let result = pending.interval(Duration::from_millis(1000)).await;
            let _ = stream
                .send(Withdraw(WithdrawStatus::Submitted { tx_hash }))
                .await;
            result
        }
        Err(e) => {
            tracing::error!("Error while sending Tx: {}", e);
            let err = into_withdraw_error(e);
            let _ = stream.send(Withdraw(err)).await;
            return;
        }
    };
    match tx {
        Ok(Some(receipt)) => {
            tracing::debug!("Finalized Tx #{}", receipt.transaction_hash);
            let _ = stream
                .send(Withdraw(WithdrawStatus::Finalized {
                    tx_hash: receipt.transaction_hash,
                }))
                .await;
        }
        Ok(None) => {
            tracing::warn!("Transaction Dropped from Mempool!!");
            let _ = stream
                .send(Withdraw(WithdrawStatus::DroppedFromMemPool))
                .await;
        }
        Err(e) => {
            let reason = e.to_string();
            tracing::error!("Transaction Errored: {}", reason);
            let _ = stream
                .send(Withdraw(WithdrawStatus::Errored { reason, code: 4 }))
                .await;
        }
    };
}
