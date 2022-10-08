use crate::{
    context::RelayerContext,
    handler::{CommandResponse, CommandStream, SubstrateCommand},
    tx_relay::substrate::handle_substrate_tx,
};
use webb::substrate::protocol_substrate_runtime::api as RuntimeApi;
use webb::substrate::{
    protocol_substrate_runtime::api::runtime_types::webb_primitives::runtime::Element,
    subxt::{tx::PairSigner, SubstrateConfig},
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
        SubstrateCommand::Mixer(cmd) => cmd,
        _ => return,
    };

    let root_element = Element(cmd.root);
    let nullifier_hash_element = Element(cmd.nullifier_hash);

    let requested_chain = cmd.chain_id;
    let maybe_client = ctx
        .substrate_provider::<SubstrateConfig>(&requested_chain.to_string())
        .await;
    let client = match maybe_client {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("Error while getting Substrate client: {}", e);
            let _ = stream.send(Error(format!("{}", e))).await;
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

    let event_stream = match withdraw_tx_hash {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Error while sending Tx: {}", e);
            let _ = stream.send(Error(format!("{}", e))).await;
            return;
        }
    };

    handle_substrate_tx(event_stream, stream).await;
}
