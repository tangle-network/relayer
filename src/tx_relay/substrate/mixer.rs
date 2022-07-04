use webb::substrate::{
    protocol_substrate_runtime::api::{
        runtime_types::webb_primitives::runtime::Element, RuntimeApi,
    },
    subxt::{self, DefaultConfig, PairSigner},
};

use crate::{
    context::RelayerContext,
    handler::{CommandResponse, CommandStream, SubstrateCommand},
    tx_relay::substrate::handle_substrate_tx,
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
    let api = client.to_runtime_api::<RuntimeApi<
        DefaultConfig,
        subxt::SubstrateExtrinsicParams<DefaultConfig>,
    >>();

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

    let withdraw_tx = api.tx().mixer_bn254().withdraw(
        cmd.id,
        cmd.proof,
        root_element,
        nullifier_hash_element,
        cmd.recipient,
        cmd.relayer,
        cmd.fee,
        cmd.refund,
    );
    let withdraw_tx = match withdraw_tx {
        Ok(tx) => tx.sign_and_submit_then_watch_default(&signer).await,
        Err(e) => {
            tracing::error!(
                "Error while signing and submitting mixer withdraw tx: {}",
                e
            );
            let _ = stream
                .send(Error(format!(
                    "Error while signing and submitting mixer withdraw tx: {}",
                    e
                )))
                .await;
            return;
        }
    };

    let event_stream = match withdraw_tx {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("Error while sending Tx: {}", e);
            let _ = stream.send(Error(format!("{}", e))).await;
            return;
        }
    };

    handle_substrate_tx(event_stream, stream).await;
}
