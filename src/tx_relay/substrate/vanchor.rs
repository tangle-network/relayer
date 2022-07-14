use webb::substrate::subxt::sp_runtime::AccountId32;
use webb::substrate::{
    protocol_substrate_runtime::api::{
        runtime_types::{
            webb_primitives::runtime::Element, webb_primitives::types::vanchor,
        },
        RuntimeApi,
    },
    subxt::{self, DefaultConfig, PairSigner},
};

use crate::handler::SubstrateCommand;
use crate::tx_relay::substrate::handle_substrate_tx;
use crate::{
    context::RelayerContext,
    handler::{CommandResponse, CommandStream},
};

/// Handler for Substrate Anchor commands
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `cmd` - The command to execute
/// * `stream` - The stream to write the response to
pub async fn handle_substrate_vanchor_relay_tx<'a>(
    ctx: RelayerContext,
    cmd: SubstrateCommand,
    stream: CommandStream,
) {
    use CommandResponse::*;
    let cmd = match cmd {
        SubstrateCommand::VAnchor(cmd) => cmd,
        _ => return,
    };

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
    let ext_data_elements: vanchor::ExtData<AccountId32, i128, u128> =
        vanchor::ExtData {
            recipient: cmd.ext_data.recipient,
            relayer: cmd.ext_data.relayer,
            fee: cmd.ext_data.fee,
            ext_amount: cmd.ext_data.ext_amount,
            encrypted_output1: cmd.ext_data.encrypted_output1.to_vec(),
            encrypted_output2: cmd.ext_data.encrypted_output2.to_vec(),
        };

    let requested_chain = cmd.chain_id;
    let maybe_client = ctx
        .substrate_provider::<DefaultConfig>(&requested_chain.to_string())
        .await;
    let client = match maybe_client {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Error while getting Substrate client: {}", e);
            let _ = stream.send(Error(format!("{}", e))).await;
            return;
        }
    };
    let api = client.to_runtime_api::<RuntimeApi<
        DefaultConfig,
        subxt::SubstrateExtrinsicParams<DefaultConfig>,
    >>();

    let pair = match ctx.substrate_wallet(&cmd.chain_id.to_string()).await {
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

    let signer = PairSigner::new(pair);

    let transact_tx = api.tx().v_anchor_bn254().transact(
        cmd.id,
        proof_elements,
        ext_data_elements,
    );
    let transact_tx = match transact_tx {
        Ok(tx) => tx.sign_and_submit_then_watch_default(&signer).await,
        Err(e) => {
            tracing::error!("Error while creating transaction: {}", e);
            let _ = stream.send(Error(format!("{}", e))).await;
            return;
        }
    };

    let event_stream = match transact_tx {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Error while sending Tx: {}", e);
            let _ = stream.send(Error(format!("{}", e))).await;
            return;
        }
    };

    handle_substrate_tx(event_stream, stream).await;
}
