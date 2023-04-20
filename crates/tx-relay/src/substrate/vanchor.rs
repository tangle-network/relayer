use super::*;
use crate::substrate::handle_substrate_tx;
use webb::substrate::tangle_runtime::api as RuntimeApi;
use webb::substrate::subxt::utils::AccountId32;
use webb::substrate::tangle_runtime::api::runtime_types::tangle_standalone_runtime::protocol_substrate_config::Element;
use webb::substrate::{
    tangle_runtime::api::runtime_types::{
    webb_primitives::types::vanchor,
    },
    subxt::{tx::PairSigner, PolkadotConfig},
};
use webb_proposals::{
    ResourceId, SubstrateTargetSystem, TargetSystem, TypedChainId,
};
use webb_relayer_context::RelayerContext;
use webb_relayer_handler_utils::SubstrateVAchorCommand;
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
    cmd: SubstrateVAchorCommand,
    stream: CommandStream,
) -> Result<(), CommandResponse> {
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
    let ext_data_elements: vanchor::ExtData<AccountId32, i128, u128, _> =
        vanchor::ExtData {
            recipient: cmd.ext_data.recipient,
            relayer: cmd.ext_data.relayer,
            fee: cmd.ext_data.fee,
            ext_amount: cmd.ext_data.ext_amount,
            encrypted_output1: cmd.ext_data.encrypted_output1.clone(),
            encrypted_output2: cmd.ext_data.encrypted_output2.clone(),
            refund: cmd.ext_data.refund,
            token: cmd.ext_data.token,
        };

    let requested_chain = cmd.chain_id;
    let maybe_client = ctx
        .substrate_provider::<PolkadotConfig>(&requested_chain.to_string())
        .await;
    let client = maybe_client.map_err(|e| {
        Error(format!("Error while getting Substrate client: {e}"))
    })?;

    let pair = ctx
        .substrate_wallet(&cmd.chain_id.to_string())
        .await
        .map_err(|e| {
            Error(format!("Misconfigured Network {:?}: {e}", cmd.chain_id))
        })?;

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

    let event_stream = transact_tx_hash
        .map_err(|e| Error(format!("Error while sending Tx: {e}")))?;

    handle_substrate_tx(event_stream, stream, cmd.chain_id).await?;

    let target = client
        .metadata()
        .pallet("VAnchorHandlerBn254")
        .map(|pallet| {
            SubstrateTargetSystem::builder()
                .pallet_index(pallet.index())
                .tree_id(cmd.id)
                .build()
        })
        .map_err(|e| Error(format!("Vanchor handler pallet not found: {e}")))?;

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
    Ok(())
}
