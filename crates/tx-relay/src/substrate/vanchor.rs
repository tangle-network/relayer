use super::*;
use crate::substrate::handle_substrate_tx;
use webb::substrate::protocol_substrate_runtime::api as RuntimeApi;
use webb::substrate::subxt::ext::sp_runtime::AccountId32;
use webb::substrate::{
    protocol_substrate_runtime::api::runtime_types::{
        webb_primitives::runtime::Element, webb_primitives::types::vanchor,
    },
    subxt::{tx::PairSigner, SubstrateConfig},
};
use webb_proposals::{
    ResourceId, SubstrateTargetSystem, TargetSystem, TypedChainId,
};
use webb_relayer_context::RelayerContext;
use webb_relayer_handler_utils::SubstrateCommand;
use webb_relayer_utils::metric::Metrics;

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
    let ext_data_elements: vanchor::ExtData<AccountId32, i128, u128, _> =
        vanchor::ExtData {
            recipient: cmd.ext_data.recipient,
            relayer: cmd.ext_data.relayer,
            fee: cmd.ext_data.fee,
            ext_amount: cmd.ext_data.ext_amount,
            encrypted_output1: cmd.ext_data.encrypted_output1.to_vec(),
            encrypted_output2: cmd.ext_data.encrypted_output2.to_vec(),
            refund: cmd.ext_data.refund,
            token: cmd.ext_data.token,
        };

    let requested_chain = cmd.chain_id;
    let maybe_client = ctx
        .substrate_provider::<SubstrateConfig>(&requested_chain.to_string())
        .await;
    let client = match maybe_client {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Error while getting Substrate client: {}", e);
            let _ = stream.send(Error(format!("{e}"))).await;
            return;
        }
    };

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

    let transact_tx = RuntimeApi::tx().v_anchor_bn254().transact(
        cmd.id,
        proof_elements,
        ext_data_elements,
    );
    let transact_tx_hash = client
        .tx()
        .sign_and_submit_then_watch_default(&transact_tx, &signer)
        .await;

    let event_stream = match transact_tx_hash {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Error while sending Tx: {}", e);
            let _ = stream.send(Error(format!("{e}"))).await;
            return;
        }
    };

    handle_substrate_tx(event_stream, stream, cmd.chain_id).await;

    let pallet_index = {
        let metadata = client.metadata();
        let pallet = metadata.pallet("VAnchorHandlerBn254").unwrap();
        pallet.index()
    };
    let target = SubstrateTargetSystem::builder()
        .pallet_index(pallet_index)
        .tree_id(cmd.id)
        .build();
    let target_system = TargetSystem::Substrate(target);
    let typed_chain_id = TypedChainId::Substrate(cmd.chain_id as u32);
    let resource_id = ResourceId::new(target_system, typed_chain_id);

    // update metric
    let metrics_clone = ctx.metrics.clone();
    let mut metrics = metrics_clone.lock().await;
    // update metric for total fee earned by relayer on particular resource
    let resource_metric = metrics
        .resource_metric_map
        .entry(resource_id)
        .or_insert_with(|| Metrics::register_resource_id_counters(resource_id));

    resource_metric
        .total_fee_earned
        .inc_by(cmd.ext_data.fee as f64);
    // update metric for total fee earned by relayer
    metrics.total_fee_earned.inc_by(cmd.ext_data.fee as f64);
}
