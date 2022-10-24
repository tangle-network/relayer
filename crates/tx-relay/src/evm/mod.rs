use std::time::Duration;

use webb::evm::ethers::{
    abi::Detokenize,
    prelude::{builders::ContractCall, Middleware},
};
use webb_relayer_handler_utils::{
    into_withdraw_error, CommandResponse, CommandStream, WithdrawStatus,
};

/// Variable Anchor transaction relayer.
pub mod vanchor;

/// Submits a dry-run and then submits the actual transaction for an EVM transaction.
///
/// This is meant to be reused amongst all kinds of EVM transactions that the relayer sends.
/// The intention is that a dry-run call is made first to ensure that the transaction is valid
/// and then the actual transaction is submitted and its progress is monitored.
pub async fn handle_evm_tx<M, D>(
    call: ContractCall<M, D>,
    stream: CommandStream,
) where
    M: Middleware,
    D: Detokenize,
{
    use CommandResponse::*;
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
