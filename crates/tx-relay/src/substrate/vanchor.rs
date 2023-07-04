use std::sync::Arc;

use super::*;
use crate::TransactionItemKey;
use crate::substrate::fees::get_substrate_fee_info;
use webb::evm::ethers::utils::hex;
use webb::substrate::tangle_runtime::api as RuntimeApi;
use webb::substrate::subxt::utils::AccountId32;
use webb::substrate::tangle_runtime::api::runtime_types::tangle_standalone_runtime::protocol_substrate_config::Element;
use webb::substrate::{
    subxt::{PolkadotConfig, tx::PairSigner},
    tangle_runtime::api::runtime_types::webb_primitives::types::vanchor,
};
use ethereum_types::{U256, H512};
use webb_proposals::{
    ResourceId, SubstrateTargetSystem, TargetSystem, TypedChainId,
};
use webb_relayer_context::RelayerContext;
use webb_relayer_handler_utils::SubstrateVAchorCommand;
use webb_relayer_store::queue::{QueueItem, TransactionQueueItemKey, QueueStore};
use webb_relayer_store::sled::SledQueueKey;
use webb_relayer_utils::TransactionRelayingError;
use webb_relayer_utils::static_tx_payload::TypeErasedStaticTxPayload;

/// Handler for Substrate Anchor commands
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `cmd` - The command to execute
pub async fn handle_substrate_vanchor_relay_tx<'a>(
    ctx: Arc<RelayerContext>,
    cmd: SubstrateVAchorCommand,
) -> Result<TransactionItemKey, TransactionRelayingError> {
    use TransactionRelayingError::*;
    let proof_elements: vanchor::ProofData<Element> = vanchor::ProofData {
        proof: cmd.proof_data.proof.to_vec(),
        public_amount: Element(cmd.proof_data.public_amount.to_fixed_bytes()),
        roots: cmd
            .proof_data
            .roots
            .iter()
            .map(|r| Element(r.to_fixed_bytes()))
            .collect(),
        input_nullifiers: cmd
            .proof_data
            .input_nullifiers
            .iter()
            .map(|r| Element(r.to_fixed_bytes()))
            .collect(),
        output_commitments: cmd
            .proof_data
            .output_commitments
            .iter()
            .map(|r| Element(r.to_fixed_bytes()))
            .collect(),
        ext_data_hash: Element(cmd.proof_data.ext_data_hash.to_fixed_bytes()),
    };
    let ext_data_elements: vanchor::ExtData<AccountId32, i128, u128, _> =
        vanchor::ExtData {
            recipient: cmd.ext_data.recipient.to_fixed_bytes().into(),
            relayer: cmd.ext_data.relayer.to_fixed_bytes().into(),
            fee: cmd.ext_data.fee.as_u128(),
            ext_amount: cmd.ext_data.ext_amount.0,
            encrypted_output1: cmd.ext_data.encrypted_output1.to_vec(),
            encrypted_output2: cmd.ext_data.encrypted_output2.to_vec(),
            refund: cmd.ext_data.refund.as_u128(),
            token: cmd.ext_data.token,
        };
    let requested_chain = cmd.chain_id;
    let chain_config = ctx
        .config
        .evm
        .get(&requested_chain.to_string())
        .ok_or(UnsupportedChain(requested_chain))?;
    let maybe_client = ctx
        .substrate_provider::<PolkadotConfig, _>(requested_chain)
        .await;
    let client = maybe_client.map_err(|e| {
        ClientError(format!("Error while getting Substrate client: {e}"))
    })?;

    let pair = ctx
        .substrate_wallet(requested_chain)
        .await
        .map_err(|e| NetworkConfigurationError(e.to_string(), cmd.chain_id))?;

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
        .map_err(|e| ClientError(format!("Failed to sign transaction: {e}")))?;
    let estimated_fee = signed
        .partial_fee_estimate()
        .await
        .map_err(|e| ClientError(format!("Failed to estimate fee: {e}")))?;
    let fee_info = get_substrate_fee_info(
        requested_chain,
        U256::from(estimated_fee),
        &ctx,
    )
    .await
    .map_err(|e| ClientError(format!("Get substrate fee info failed: {e}")))?;

    // validate refund amount
    if U256::from(cmd.ext_data.refund) > fee_info.max_refund {
        // TODO: use error enum for these messages so they dont have to be duplicated between
        //       evm/substrate
        let msg = format!(
            "User requested a refund which is higher than the maximum of {}",
            fee_info.max_refund
        );
        return Err(InvalidRefundAmount(msg));
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
        return Err(InvalidRefundAmount(msg));
    }

    let typed_tx =
        TypeErasedStaticTxPayload::try_from(transact_tx).map_err(|e| {
            ClientError(format!("Failed to construct tx payload: {e}"))
        })?;

    let item = QueueItem::new(typed_tx.clone());
    let tx_key = SledQueueKey::from_substrate_with_custom_key(
        chain_config.chain_id,
        typed_tx.item_key(),
    );
    let store = ctx.store();
    QueueStore::<TypeErasedStaticTxPayload>::enqueue_item(
        store,
        tx_key,
        item.clone(),
    )
    .map_err(|_| {
        TransactionQueueError(format!(
            "Transaction item with key : {} failed to enqueue",
            tx_key
        ))
    })?;

    tracing::trace!(
            tx_call = %hex::encode(typed_tx.clone().call_data),
            "Enqueued private withdraw transaction call for execution through substrate tx queue",
    );

    let item_key_hex = H512::from_slice(typed_tx.item_key().as_slice());

    let target = client
        .metadata()
        .pallet_by_name_err("VAnchorHandlerBn254")
        .map(|pallet| {
            SubstrateTargetSystem::builder()
                .pallet_index(pallet.index())
                .tree_id(cmd.id)
                .build()
        })
        .map_err(|e| {
            ClientError(format!("Vanchor handler pallet not found: {e}"))
        })?;

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

    let balance = balance(client, signer).await.map_err(|e| {
        ClientError(format!("Failed to read substrate balance: {e}"))
    })?;
    metrics
        .account_balance_entry(typed_chain_id)
        .set(wei_to_gwei(balance));
    Ok(item_key_hex)
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
            .at_latest()
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
