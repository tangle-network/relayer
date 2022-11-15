use ethereum_types::H256;
use futures::StreamExt;
use webb::substrate::subxt::{
    tx::TxProgress, tx::TxStatus as TransactionStatus, OnlineClient,
    SubstrateConfig,
};
use webb_relayer_handler_utils::{
    CommandResponse, CommandStream, WithdrawStatus,
};

/// Substrate Mixer Transactional Relayer.
pub mod mixer;
/// Substrate Variable Anchor Transactional Relayer.
pub mod vanchor;

/// Handles a submitted Substrate transaction by processing its `TransactionProgress`.
///
/// The `TransactionProgress` is a subscription to a transaction's progress. This method
/// is intended to be used in a variety of places for all kinds of submitted Substrate
/// transactions.
pub async fn handle_substrate_tx(
    mut event_stream: TxProgress<
        SubstrateConfig,
        OnlineClient<SubstrateConfig>,
    >,
    stream: CommandStream,
    chain_id: u64,
) {
    use CommandResponse::*;
    // Listen to the withdraw transaction, and send information back to the client
    loop {
        let maybe_event = event_stream.next().await;
        let event = match maybe_event {
            Some(Ok(v)) => v,
            Some(Err(e)) => {
                tracing::error!("Error while watching Tx: {}", e);
                let _ = stream.send(Error(format!("{e}"))).await;
                return;
            }
            None => break,
        };
        match event {
            TransactionStatus::Broadcast(_) => {
                let _ = stream.send(Withdraw(WithdrawStatus::Sent)).await;
            }
            TransactionStatus::InBlock(info) => {
                tracing::event!(
                    target: webb_relayer_utils::probe::TARGET,
                    tracing::Level::DEBUG,
                    kind = %webb_relayer_utils::probe::Kind::TxRelay,
                    ty = "SUBSTRATE",
                    chain_id = %chain_id,
                    status = "InBlock",
                    block_hash = %info.block_hash(),
                );

                let _ = stream
                    .send(Withdraw(WithdrawStatus::Submitted {
                        tx_hash: H256::from_slice(
                            info.extrinsic_hash().as_ref(),
                        ),
                    }))
                    .await;
            }
            TransactionStatus::Finalized(info) => {
                tracing::event!(
                    target: webb_relayer_utils::probe::TARGET,
                    tracing::Level::DEBUG,
                    kind = %webb_relayer_utils::probe::Kind::TxRelay,
                    ty = "SUBSTRATE",
                    chain_id = %chain_id,
                    status = "Finalized",
                    finalized = true,
                    block_hash = %info.block_hash(),
                );
                let _has_event = match info.wait_for_success().await {
                    Ok(_) => {
                        // TODO: check if the event is actually a withdraw event
                        true
                    }
                    Err(e) => {
                        tracing::error!("Error while watching Tx: {}", e);
                        let _ = stream.send(Error(format!("{e}"))).await;
                        false
                    }
                };
                let _ = stream
                    .send(Withdraw(WithdrawStatus::Finalized {
                        tx_hash: H256::from_slice(
                            info.extrinsic_hash().as_ref(),
                        ),
                    }))
                    .await;
            }
            TransactionStatus::Dropped => {
                tracing::event!(
                    target: webb_relayer_utils::probe::TARGET,
                    tracing::Level::DEBUG,
                    kind = %webb_relayer_utils::probe::Kind::TxRelay,
                    ty = "SUBSTRATE",
                    chain_id = %chain_id,
                    status = "Dropped",
                );
                let _ = stream
                    .send(Withdraw(WithdrawStatus::DroppedFromMemPool))
                    .await;
            }
            TransactionStatus::Invalid => {
                tracing::event!(
                    target: webb_relayer_utils::probe::TARGET,
                    tracing::Level::DEBUG,
                    kind = %webb_relayer_utils::probe::Kind::TxRelay,
                    ty = "SUBSTRATE",
                    chain_id = %chain_id,
                    status = "Invalid",
                );
                let _ = stream
                    .send(Withdraw(WithdrawStatus::Errored {
                        reason: "Invalid".to_string(),
                        code: 4,
                    }))
                    .await;
            }
            _ => continue,
        }
    }
}
