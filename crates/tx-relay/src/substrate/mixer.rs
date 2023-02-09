use super::*;
use crate::substrate::handle_substrate_tx;
use webb::substrate::protocol_substrate_runtime::api as RuntimeApi;
use webb::substrate::{
    protocol_substrate_runtime::api::runtime_types::webb_primitives::runtime::Element,
    subxt::{tx::PairSigner, SubstrateConfig},
};
use webb_relayer_context::RelayerContext;
use webb_relayer_handler_utils::SubstrateCommand;

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
) -> Result<(), CommandResponse> {
    use CommandResponse::*;

    let cmd = match cmd {
        SubstrateCommand::Mixer(cmd) => cmd,
        _ => return Err(Unimplemented("Unsupported command")),
    };

    let root_element = Element(cmd.root);
    let nullifier_hash_element = Element(cmd.nullifier_hash);

    let requested_chain = cmd.chain_id;
    let client = ctx
        .substrate_provider::<SubstrateConfig>(&requested_chain.to_string())
        .await
        .map_err(|e| {
            Error(format!("Error while getting Substrate client: {e}"))
        })?;

    let pair = ctx
        .substrate_wallet(&cmd.chain_id.to_string())
        .await
        .map_err(|e| {
            Error(format!("Misconfigured Network {:?}: {e}", cmd.chain_id))
        })?;

    let signer = PairSigner::new(pair);

    let withdraw_tx = RuntimeApi::tx().mixer_bn254().withdraw(
        cmd.id,
        cmd.proof,
        root_element,
        nullifier_hash_element,
        cmd.recipient,
        cmd.relayer,
        cmd.fee,
        cmd.refund,
    );

    let withdraw_tx_hash = client
        .tx()
        .sign_and_submit_then_watch_default(&withdraw_tx, &signer)
        .await;

    let event_stream = withdraw_tx_hash
        .map_err(|e| Error(format!("Error while sending Tx: {e}")))?;

    handle_substrate_tx(event_stream, stream, cmd.chain_id).await?;
    Ok(())
}
