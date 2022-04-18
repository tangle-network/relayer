use std::sync::Arc;
use std::{collections::HashMap, time::Duration};

use ethereum_types::U256;
use webb::evm::{
    contract::tornado::TornadoContract,
    ethers::prelude::{Signer, SignerMiddleware},
};

use crate::handler::EvmCommand;
use crate::{
    context::RelayerContext,
    handler::{
        calculate_fee, into_withdraw_error, NetworkStatus, WithdrawStatus,
    },
    handler::{CommandResponse, CommandStream},
};

/// Handler for tornado mixer commands
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `cmd` - The command to execute
/// * `stream` - The stream to write the response to
pub async fn handle_mixer_relay_tx<'a>(
    ctx: RelayerContext,
    cmd: EvmCommand,
    stream: CommandStream,
) {
    use CommandResponse::*;
    let cmd = match cmd {
        EvmCommand::MixerRelayTx(cmd) => cmd,
        _ => return,
    };

    let requested_chain = cmd.chain.to_lowercase();
    let chain = match ctx.config.evm.get(&requested_chain) {
        Some(v) => v,
        None => {
            tracing::event!(
                target: crate::probe::TARGET,
                tracing::Level::DEBUG,
                kind = %crate::probe::Kind::RelayTx,
                network = ?NetworkStatus::UnsupportedChain,
            );
            let _ = stream.send(Network(NetworkStatus::UnsupportedChain)).await;
            return;
        }
    };
    let supported_contracts: HashMap<_, _> = chain
        .contracts
        .iter()
        .cloned()
        .filter_map(|c| match c {
            crate::config::Contract::Tornado(c) => Some(c),
            _ => None,
        })
        .map(|c| (c.common.address, c))
        .collect();
    // get the contract configuration
    let contract_config = match supported_contracts.get(&cmd.id) {
        Some(config) => config,
        None => {
            tracing::event!(
                target: crate::probe::TARGET,
                tracing::Level::DEBUG,
                kind = %crate::probe::Kind::RelayTx,
                network = ?NetworkStatus::UnsupportedContract,
            );
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
            tracing::event!(
                target: crate::probe::TARGET,
                tracing::Level::DEBUG,
                kind = %crate::probe::Kind::RelayTx,
                network = ?NetworkStatus::Misconfigured,
            );
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
        tracing::event!(
            target: crate::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %crate::probe::Kind::RelayTx,
            network = ?NetworkStatus::InvalidRelayerAddress,
        );
        let _ = stream
            .send(Network(NetworkStatus::InvalidRelayerAddress))
            .await;
        return;
    }

    tracing::debug!(
        "Connecting to chain {:?} .. at {}",
        cmd.chain,
        chain.http_endpoint
    );
    tracing::event!(
        target: crate::probe::TARGET,
        tracing::Level::DEBUG,
        kind = %crate::probe::Kind::RelayTx,
        network = ?NetworkStatus::Connecting,
    );
    let _ = stream.send(Network(NetworkStatus::Connecting)).await;
    let provider = match ctx.evm_provider(&cmd.chain).await {
        Ok(value) => {
            tracing::event!(
                target: crate::probe::TARGET,
                tracing::Level::DEBUG,
                kind = %crate::probe::Kind::RelayTx,
                network = ?NetworkStatus::Connected,
            );
            let _ = stream.send(Network(NetworkStatus::Connected)).await;
            value
        }
        Err(e) => {
            let reason = e.to_string();
            tracing::event!(
                target: crate::probe::TARGET,
                tracing::Level::DEBUG,
                kind = %crate::probe::Kind::RelayTx,
                network = "Failed",
                %reason,
            );
            let _ =
                stream.send(Network(NetworkStatus::Failed { reason })).await;
            tracing::event!(
                target: crate::probe::TARGET,
                tracing::Level::DEBUG,
                kind = %crate::probe::Kind::RelayTx,
                network = ?NetworkStatus::Disconnected,
            );
            let _ = stream.send(Network(NetworkStatus::Disconnected)).await;
            return;
        }
    };

    let client = SignerMiddleware::new(provider, wallet);
    let client = Arc::new(client);
    let contract = TornadoContract::new(cmd.id, client);
    let denomination = match contract.denomination().call().await {
        Ok(v) => v,
        Err(e) => {
            tracing::event!(
                target: crate::probe::TARGET,
                tracing::Level::DEBUG,
                kind = %crate::probe::Kind::RelayTx,
                network = "Failed",
                reason = "Failed to get denomination from contract",
            );
            tracing::error!("Misconfigured Contract Denomination: {}", e);
            let _ = stream
                .send(Error(format!(
                    "Misconfigured Contract Denomination: {:?}",
                    e
                )))
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

    let call = contract.withdraw(
        cmd.proof,
        cmd.root.to_fixed_bytes(),
        cmd.nullifier_hash.to_fixed_bytes(),
        cmd.recipient,
        cmd.relayer,
        cmd.fee,
        cmd.refund,
    );
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
            tracing::debug!("Tx is submitted and pending! {}", tx_hash);
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
