use super::*;
use crate::substrate::fees::get_substrate_fee_info;
use crate::substrate::handle_substrate_tx;
use webb::substrate::tangle_runtime::api as RuntimeApi;
use webb::substrate::subxt::utils::AccountId32;
use webb::substrate::tangle_runtime::api::runtime_types::tangle_standalone_runtime::protocol_substrate_config::Element;
use webb::substrate::{
    subxt::{PolkadotConfig, tx::PairSigner},
    tangle_runtime::api::runtime_types::webb_primitives::types::vanchor,
};use ethereum_types::U256;
use sp_core::{Decode, Encode};
use webb::substrate::scale::Compact;
use webb_proposals::{
    ResourceId, SubstrateTargetSystem, TargetSystem, TypedChainId,
};
use webb_relayer_context::RelayerContext;
use webb_relayer_handler_utils::SubstrateVAchorCommand;

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
            fee: cmd.ext_data.fee.as_u128(),
            ext_amount: cmd.ext_data.ext_amount.0,
            encrypted_output1: cmd.ext_data.encrypted_output1.clone(),
            encrypted_output2: cmd.ext_data.encrypted_output2.clone(),
            refund: cmd.ext_data.refund.as_u128(),
            token: cmd.ext_data.token,
        };

    let requested_chain = cmd.chain_id;
    let maybe_client = ctx
        .substrate_provider::<PolkadotConfig, _>(requested_chain)
        .await;
    let client = maybe_client.map_err(|e| {
        Error(format!("Error while getting Substrate client: {e}"))
    })?;

    let pair = ctx
        .substrate_wallet(requested_chain)
        .await
        .map_err(|e| {
            Error(format!("Misconfigured Network {:?}: {e}", cmd.chain_id))
        })?;

    let signer = PairSigner::new(pair.clone());

    let transact_tx = RuntimeApi::tx().v_anchor_bn254().transact(
        cmd.id,
        proof_elements,
        ext_data_elements,
    );

    // TODO: Taken from subxt PR. Replace with new method state_call_decoded() after upgrading subxt.
    //       https://github.com/paritytech/subxt/pull/910
    let signed = client
        .tx()
        .create_signed(&transact_tx, &signer, Default::default())
        .await
        .map_err(|e| Error(format!("Failed to sign transaction: {e}")))?;
    let mut params = signed.encoded().to_vec();
    (signed.encoded().len() as u32).encode_to(&mut params);
    let bytes = client
        .rpc()
        .state_call("TransactionPaymentApi_query_info", Some(&params), None)
        .await
        .map_err(|e| {
            Error(format!(
                "RPC call TransactionPaymentApi_query_info failed: {e}"
            ))
        })?;
    let cursor = &mut &bytes[..];
    let payment_info: (Compact<u64>, Compact<u64>, u8, u128) =
        Decode::decode(cursor).map_err(|e| {
            Error(format!("Failed to decode payment info: {e}"))
        })?;
    let fee_info = get_substrate_fee_info(
        requested_chain,
        U256::from(payment_info.3),
        &ctx,
    )
    .await
    .map_err(|e| Error(format!("Get substrate fee info failed: {e}")))?;

    // validate refund amount
    if U256::from(cmd.ext_data.refund) > fee_info.max_refund {
        // TODO: use error enum for these messages so they dont have to be duplicated between
        //       evm/substrate
        let msg = format!(
            "User requested a refund which is higher than the maximum of {}",
            fee_info.max_refund
        );
        return Err(Error(msg));
    }

    // Check that transaction fee is enough to cover network fee and relayer fee
    // TODO: refund needs to be converted from wrapped token to native token once there
    //       is an exchange rate
    if U256::from(cmd.ext_data.fee)
        < fee_info.estimated_fee + cmd.ext_data.refund
    {
        let msg = format!(
            "User sent a fee that is too low ({}) but expected {}",
            cmd.ext_data.fee,
            fee_info.estimated_fee + cmd.ext_data.refund
        );
        return Err(Error(msg));
    }

    let transact_tx_hash = signed.submit_and_watch().await;

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
    metrics
        .resource_metric_entry(resource_id)
        .total_fee_earned
        .inc_by(cmd.ext_data.fee.as_u128() as f64);
    // update metric for total fee earned by relayer
    metrics
        .total_fee_earned
        .inc_by(wei_to_gwei(cmd.ext_data.fee.as_u128()));

    let balance = balance(client, signer)
        .await
        .map_err(|e| Error(format!("Failed to read substrate balance: {e}")))?;
    metrics
        .account_balance_entry(typed_chain_id)
        .set(wei_to_gwei(balance));
    Ok(())
}

#[cfg(tests)]
mod test {
    use webb::substrate::subxt::runtime_api::RuntimeApiClient;
    use webb::substrate::subxt::tx::PairSigner;
    use webb::substrate::subxt::utils::AccountId32;
    use webb::substrate::subxt::SubstrateConfig;

    #[tokio::test]
    async fn test_account_balance() {
        // TODO: use alice account id from
        // https://docs.rs/sp-keyring/latest/sp_keyring/ed25519/enum.Keyring.html
        let account_id = AccountId32::from([0u8; 32]);
        let account = RuntimeApi::storage().balances().account(account_id);
        let client = subxt::OnlineClient::<SubstrateConfig>::default().await?;
        let balance = client
            .storage()
            .at(None)
            .await
            .unwrap()
            .fetch(&account)
            .await
            .unwrap();
        dbg!(balance);
        assert_eq!(balance, None);
        fail!();
    }
}
