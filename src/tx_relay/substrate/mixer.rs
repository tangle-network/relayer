use ethereum_types::H256;
use tokio_stream::StreamExt;
use webb::substrate::{
    protocol_substrate_runtime::api::{
        runtime_types::webb_standalone_runtime::Element, RuntimeApi,
    },
    subxt::{self, DefaultConfig, PairSigner, TransactionStatus},
};

use crate::{
    context::RelayerContext,
    handler::{CommandResponse, CommandStream, SubstrateCommand},
    handler::{WithdrawStatus},
};

/// Handler for Substrate Mixer commands
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `cmd` - The command to execute
/// * `stream` - The stream to write the response to
pub async fn handle_substrate_mixer_relay_tx<'a>(
    ctx: RelayerContext,
    cmd: SubstrateCommand,
    stream: CommandStream,
) {
    use CommandResponse::*;

    let cmd = match cmd {
        SubstrateCommand::MixerRelayTx(cmd) => cmd,
        _ => return
    };

    let root_element = Element(cmd.root);
    let nullifier_hash_element = Element(cmd.nullifier_hash);

    let requested_chain = cmd.chain.to_lowercase();
    let maybe_client = ctx
        .substrate_provider::<DefaultConfig>(&requested_chain)
        .await;
    let client = match maybe_client {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Error while getting Substrate client: {}", e);
            let _ = stream.send(Error(format!("{}", e))).await;
            return;
        }
    };
    let api = client.to_runtime_api::<RuntimeApi<DefaultConfig, subxt::DefaultExtra<DefaultConfig>>>();

    let pair = match ctx.substrate_wallet(&cmd.chain).await {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("Misconfigured Network: {}", e);
            let _ = stream
                .send(Error(format!("Misconfigured Network: {:?}", cmd.chain)))
                .await;
            return;
        }
    };

    let signer = PairSigner::new(pair);

    let withdraw_tx = api
        .tx()
        .mixer_bn254()
        .withdraw(
            cmd.id,
            cmd.proof,
            root_element,
            nullifier_hash_element,
            cmd.recipient,
            cmd.relayer,
            cmd.fee,
            cmd.refund,
        )
        .sign_and_submit_then_watch(&signer)
        .await;
    let mut event_stream = match withdraw_tx {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Error while sending Tx: {}", e);
            let _ = stream.send(Error(format!("{}", e))).await;
            return;
        }
    };

    // Listen to the withdraw transaction, and send information back to the client
    loop {
        let maybe_event = event_stream.next().await;
        let event = match maybe_event {
            Some(Ok(v)) => v,
            Some(Err(e)) => {
                tracing::error!("Error while watching Tx: {}", e);
                let _ = stream.send(Error(format!("{}", e))).await;
                return;
            }
            None => break,
        };
        match event {
            TransactionStatus::Broadcast(_) => {
                let _ = stream.send(Withdraw(WithdrawStatus::Sent)).await;
            }
            TransactionStatus::InBlock(info) => {
                tracing::debug!(
                    "Transaction {:?} made it into block {:?}",
                    info.extrinsic_hash(),
                    info.block_hash()
                );
                let _ = stream
                    .send(Withdraw(WithdrawStatus::Submitted {
                        tx_hash: H256::from_slice(
                            info.extrinsic_hash().as_bytes(),
                        ),
                    }))
                    .await;
            }
            TransactionStatus::Finalized(info) => {
                tracing::debug!(
                    "Transaction {:?} finalized in block {:?}",
                    info.extrinsic_hash(),
                    info.block_hash()
                );
                let _has_event = match info.wait_for_success().await {
                    Ok(_) => {
                        // TODO: check if the event is actually a withdraw event
                        true
                    }
                    Err(e) => {
                        tracing::error!("Error while watching Tx: {}", e);
                        let _ = stream.send(Error(format!("{}", e))).await;
                        false
                    }
                };
                let _ = stream
                    .send(Withdraw(WithdrawStatus::Finalized {
                        tx_hash: H256::from_slice(
                            info.extrinsic_hash().as_bytes(),
                        ),
                    }))
                    .await;
            }
            TransactionStatus::Dropped => {
                tracing::warn!("Transaction dropped from the pool");
                let _ = stream
                    .send(Withdraw(WithdrawStatus::DroppedFromMemPool))
                    .await;
            }
            TransactionStatus::Invalid => {
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
