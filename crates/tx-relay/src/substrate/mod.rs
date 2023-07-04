use ethereum_types::H256;
use futures::TryStreamExt;
use sp_core::sr25519::Pair;
use webb::substrate::subxt::tx::PairSigner;
use webb::substrate::subxt::{
    tx::TxProgress, tx::TxStatus as TransactionStatus, OnlineClient,
    PolkadotConfig,
};
use webb::substrate::tangle_runtime::api;
use webb_relayer_handler_utils::{
    CommandResponse, CommandStream, WithdrawStatus,
};

pub mod fees;
/// Substrate Variable Anchor Transactional Relayer.
pub mod vanchor;

/// Handles a submitted Substrate transaction by processing its `TransactionProgress`.
///
/// The `TransactionProgress` is a subscription to a transaction's progress. This method
/// is intended to be used in a variety of places for all kinds of submitted Substrate
/// transactions.
pub async fn handle_substrate_tx(
    mut event_stream: TxProgress<PolkadotConfig, OnlineClient<PolkadotConfig>>,
    stream: CommandStream,
    chain_id: u64,
) -> Result<(), CommandResponse> {
    use CommandResponse::*;
    // Listen to the withdraw transaction, and send information back to the client
    loop {
        let maybe_event = event_stream
            .try_next()
            .await
            .map_err(|e| Error(format!("Error while watching Tx: {e}")))?;
        let event = match maybe_event {
            Some(v) => v,
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
                    kind = %webb_relayer_utils::probe::Kind::PrivateTx,
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
                    kind = %webb_relayer_utils::probe::Kind::PrivateTx,
                    ty = "SUBSTRATE",
                    chain_id = %chain_id,
                    status = "Finalized",
                    finalized = true,
                    block_hash = %info.block_hash(),
                );
                info.wait_for_success().await.map_err(|e| {
                    Error(format!("Error while watching Tx: {e}"))
                })?;
                // TODO: check if the event is actually a withdraw event

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
                    kind = %webb_relayer_utils::probe::Kind::PrivateTx,
                    ty = "SUBSTRATE",
                    chain_id = %chain_id,
                    status = "Dropped",
                );
                return Err(Withdraw(WithdrawStatus::DroppedFromMemPool));
            }
            TransactionStatus::Invalid => {
                tracing::event!(
                    target: webb_relayer_utils::probe::TARGET,
                    tracing::Level::DEBUG,
                    kind = %webb_relayer_utils::probe::Kind::PrivateTx,
                    ty = "SUBSTRATE",
                    chain_id = %chain_id,
                    status = "Invalid",
                );
                return Err(Withdraw(WithdrawStatus::Errored {
                    reason: "Invalid".to_string(),
                    code: 4,
                }));
            }
            _ => continue,
        }
    }
    Ok(())
}

fn wei_to_gwei(wei: u128) -> f64 {
    (wei / (10 ^ 9)) as f64
}

async fn balance(
    client: OnlineClient<PolkadotConfig>,
    signer: PairSigner<PolkadotConfig, Pair>,
) -> webb_relayer_utils::Result<u128> {
    let account = api::storage().system().account(signer.account_id());
    let balance = client
        .storage()
        .at_latest()
        .await?
        .fetch(&account)
        .await?
        .ok_or(webb_relayer_utils::Error::ReadSubstrateStorageError)?;
    Ok(balance.data.free)
}
