use ethereum_types::H256;
use serde::{Deserialize, Serialize};
use tokio_stream::StreamExt;
use webb::substrate::subxt::sp_runtime::AccountId32;
use webb::substrate::{
    protocol_substrate_runtime::api::{
        runtime_types::{
            webb_primitives::types::vanchor, webb_standalone_runtime::Element,
        },
        RuntimeApi,
    },
    subxt::{self, DefaultConfig, PairSigner, TransactionStatus},
};

use crate::{
    context::RelayerContext,
    handler::WithdrawStatus,
    handler::{CommandResponse, CommandStream},
};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ProofData<E> {
    pub proof: Vec<u8>,
    pub public_amount: E,
    pub roots: Vec<E>,
    pub input_nullifiers: Vec<E>,
    pub output_commitments: Vec<E>,
    pub ext_data_hash: E,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExtData<I, A, B, E> {
    pub recipient: I,
    pub relayer: I,
    pub ext_amount: A,
    pub fee: B,
    pub encrypted_output1: E,
    pub encrypted_output2: E,
}

/// Contains data that is relayed to the Mixers
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubstrateVAnchorRelayTransaction {
    /// one of the supported chains of this relayer
    pub chain: String,
    /// The tree id of the mixer's underlying tree
    pub id: u32,
    /// The zero-knowledge proof data structure for VAnchor transactions
    pub proof_data: ProofData<[u8; 32]>,
    /// The external data structure for arbitrary inputs
    pub ext_data: ExtData<AccountId32, i128, u128, [u8; 32]>,
}

/// Handler for Substrate Anchor commands
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `cmd` - The command to execute
/// * `stream` - The stream to write the response to
pub async fn handle_substrate_vanchor_relay_tx<'a>(
    ctx: RelayerContext,
    cmd: SubstrateVAnchorRelayTransaction,
    stream: CommandStream,
) {
    use CommandResponse::*;

    let proof_elements: vanchor::ProofData<Element> = vanchor::ProofData {
        proof: cmd.proof_data.proof,
        public_amount: Element(cmd.proof_data.public_amount),
        roots: cmd.proof_data.roots.iter().map(|r| Element(*r)).collect(),
        input_nullifiers: cmd
            .proof_data
            .input_nullifiers
            .iter()
            .map(|r| Element(*r))
            .collect(),
        output_commitments: cmd
            .proof_data
            .output_commitments
            .iter()
            .map(|r| Element(*r))
            .collect(),
        ext_data_hash: Element(cmd.proof_data.ext_data_hash),
    };
    let ext_data_elements: vanchor::ExtData<AccountId32, i128, u128, Element> =
        vanchor::ExtData {
            recipient: cmd.ext_data.recipient,
            relayer: cmd.ext_data.relayer,
            fee: cmd.ext_data.fee,
            ext_amount: cmd.ext_data.ext_amount,
            encrypted_output1: Element(cmd.ext_data.encrypted_output1),
            encrypted_output2: Element(cmd.ext_data.encrypted_output2),
        };

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

    let transact_tx = api
        .tx()
        .v_anchor_bn254()
        .transact(cmd.id, proof_elements, ext_data_elements)
        .sign_and_submit_then_watch(&signer)
        .await;
    let mut event_stream = match transact_tx {
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
