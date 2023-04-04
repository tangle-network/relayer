// Copyright 2022 Webb Technologies Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::sync::Arc;
use tokio::sync::Mutex;

use sp_core::hashing::keccak_256;
use webb::substrate::subxt::config::SubstrateConfig;
use webb::substrate::subxt::events::StaticEvent;

use webb::substrate::subxt::{self, dynamic::Value, OnlineClient};

use webb::substrate::protocol_substrate_runtime::api::signature_bridge::calls::{ExecuteProposal,SetMaintainer};
use webb_event_watcher_traits::substrate::{SubstrateBridgeWatcher, EventHandler};
use webb_event_watcher_traits::SubstrateEventWatcher;
use webb_relayer_store::sled::{SledQueueKey,SledStore};
use webb_relayer_store::{BridgeCommand, QueueStore};

use webb::evm::ethers::utils;
use webb::substrate::protocol_substrate_runtime::api as RuntimeApi;
use webb::substrate::protocol_substrate_runtime::api::signature_bridge::events::MaintainerSet;

use std::borrow::Cow;
use webb::substrate::scale::Encode;
use webb_relayer_types::dynamic_payload::WebbDynamicTxPayload;
use webb_relayer_utils::{metric, Error};

use webb::substrate::protocol_substrate_runtime::api::runtime_types::sp_core::bounded::bounded_vec::BoundedVec;

/// A MaintainerSetEvent handler handles `MaintainerSet` events and signals signature bridge watcher
/// to remove pending tx trying to do governor transfer.
#[derive(Copy, Clone, Debug, Default)]
pub struct MaintainerSetEventHandler;

#[async_trait::async_trait]
impl EventHandler<SubstrateConfig> for MaintainerSetEventHandler {
    type Client = OnlineClient<SubstrateConfig>;

    type Store = SledStore;

    async fn can_handle_events(
        &self,
        events: subxt::events::Events<SubstrateConfig>,
    ) -> webb_relayer_utils::Result<bool> {
        let has_event = events.has::<MaintainerSet>()?;
        Ok(has_event)
    }

    async fn handle_events(
        &self,
        _store: Arc<Self::Store>,
        _client: Arc<Self::Client>,
        (events, _block_number): (subxt::events::Events<SubstrateConfig>, u64),
        _metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()> {
        // todo
        // if the ownership is transferred to the new owner, we need to
        // to check our txqueue and remove any pending tx that was trying to
        // do this transfer.
        let maintainer_set_events =
            events.find::<MaintainerSet>().flatten().collect::<Vec<_>>();

        for event in maintainer_set_events {
            tracing::event!(
                target: webb_relayer_utils::probe::TARGET,
                tracing::Level::DEBUG,
                kind = %webb_relayer_utils::probe::Kind::SignatureBridge,
                call = "pallet_signature_bridge:: set_maintainer",
                msg = "Maintainer set",
                new_maintainer = ?event.new_maintainer,
                old_maintainer = ?event.old_maintainer,
            );
        }

        // mark this event as processed.
        // let events_bytes = &event.encode();
        // store.store_event(events_bytes)?;

        Ok(())
    }
}

/// A SignatureBridge watcher watches for signature bridge events and bridge commands.
#[derive(Copy, Clone, Debug, Default)]
pub struct SubstrateBridgeEventWatcher;

impl SubstrateEventWatcher<SubstrateConfig> for SubstrateBridgeEventWatcher {
    const TAG: &'static str = "Substrate bridge pallet Watcher";
    const PALLET_NAME: &'static str = MaintainerSet::PALLET;
    type Client = OnlineClient<SubstrateConfig>;
    type Store = SledStore;
}

#[async_trait::async_trait]
impl SubstrateBridgeWatcher<SubstrateConfig> for SubstrateBridgeEventWatcher {
    #[tracing::instrument(skip_all)]
    async fn handle_cmd(
        &self,
        chain_id: u32,
        store: Arc<Self::Store>,
        client: Arc<Self::Client>,
        cmd: BridgeCommand,
    ) -> webb_relayer_utils::Result<()> {
        use BridgeCommand::*;
        tracing::trace!("Got cmd {:?}", cmd);
        match cmd {
            ExecuteProposalWithSignature { data, signature } => {
                self.execute_proposal_with_signature(
                    chain_id,
                    store,
                    client.clone(),
                    (data, signature),
                )
                .await?
            }
            TransferOwnershipWithSignature {
                public_key,
                nonce,
                signature,
            } => {
                self.transfer_ownership_with_signature(
                    chain_id,
                    store,
                    client.clone(),
                    (public_key, nonce, signature),
                )
                .await?
            }
        };
        Ok(())
    }
}

impl SubstrateBridgeEventWatcher
where
    Self: SubstrateBridgeWatcher<SubstrateConfig>,
{
    #[tracing::instrument(skip_all)]
    async fn execute_proposal_with_signature(
        &self,
        chain_id: u32,
        store: Arc<<Self as SubstrateEventWatcher<SubstrateConfig>>::Store>,
        api: Arc<<Self as SubstrateEventWatcher<SubstrateConfig>>::Client>,
        (proposal_data, signature): (Vec<u8>, Vec<u8>),
    ) -> webb_relayer_utils::Result<()> {
        let proposal_data_hex = hex::encode(&proposal_data);
        // 1. Verify proposal length. Proposal length should be greater than 40 bytes (proposal header(40B) + proposal body).
        if proposal_data.len() < 40 {
            tracing::warn!(
                proposal_data = ?proposal_data_hex,
                "Skipping execution of this proposal :  Invalid Proposal",
            );
            return Ok(());
        }

        // 2. Verify proposal signature. Proposal should be signed by active maintainer/dkg-key
        let signature_hex = hex::encode(&signature);

        // get current maintainer
        let current_maintainer_addrs =
            RuntimeApi::storage().signature_bridge().maintainer();

        let current_maintainer = api
            .storage()
            .at(None)
            .await?
            .fetch(&current_maintainer_addrs)
            .await?
            .ok_or(Error::ReadSubstrateStorageError)?
            .0;

        // Verify proposal signature
        let is_signature_valid = validate_ecdsa_signature(
            proposal_data.as_slice(),
            signature.as_slice(),
            current_maintainer.as_slice(),
        )
        .unwrap_or(false);

        if !is_signature_valid {
            tracing::warn!(
                proposal_data = ?proposal_data_hex,
                signature = ?signature_hex,
                "Skipping execution of this proposal : Invalid Signature ",
            );
            return Ok(());
        }

        // 3. Enqueue proposal for execution.
        tracing::event!(
            target: webb_relayer_utils::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %webb_relayer_utils::probe::Kind::SignatureBridge,
            call = "execute_proposal_with_signature",
            chain_id = %chain_id,
            proposal_data = ?proposal_data_hex,
            signature = ?signature_hex,
        );

        let typed_chain_id = webb_proposals::TypedChainId::Substrate(chain_id);

        // Enqueue transaction call data in protocol-substrate transaction queue
        let execute_proposal_call = ExecuteProposal {
            src_id: typed_chain_id.chain_id(),
            proposal_data: BoundedVec(proposal_data.clone()),
            signature: BoundedVec(signature.clone()),
        };
        // webb dynamic payload
        let execute_proposal_tx = WebbDynamicTxPayload {
            pallet_name: Cow::Borrowed("SignatureBridge"),
            call_name: Cow::Borrowed("execute_proposal"),
            fields: vec![
                Value::u128(u128::from(typed_chain_id.chain_id())),
                Value::from_bytes(proposal_data),
                Value::from_bytes(signature),
            ],
        };

        let data_hash = utils::keccak256(execute_proposal_call.encode());
        let tx_key = SledQueueKey::from_substrate_with_custom_key(
            chain_id,
            make_execute_proposal_key(data_hash),
        );
        // Enqueue WebbDynamicTxPayload in protocol-substrate transaction queue
        QueueStore::<WebbDynamicTxPayload>::enqueue_item(
            &store,
            tx_key,
            execute_proposal_tx,
        )?;
        tracing::debug!(
            data_hash = ?hex::encode(data_hash),
            "Enqueued execute-proposal call for execution through protocol-substrate tx queue",
        );
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn transfer_ownership_with_signature(
        &self,
        chain_id: u32,
        store: Arc<<Self as SubstrateEventWatcher<SubstrateConfig>>::Store>,
        api: Arc<<Self as SubstrateEventWatcher<SubstrateConfig>>::Client>,
        (public_key, nonce, signature): (Vec<u8>, u32, Vec<u8>),
    ) -> webb_relayer_utils::Result<()> {
        let new_maintainer = public_key.clone();
        // get current maintainer
        let current_maintainer_addrs =
            RuntimeApi::storage().signature_bridge().maintainer();

        let current_maintainer = api
            .storage()
            .at(None)
            .await?
            .fetch(&current_maintainer_addrs)
            .await?
            .ok_or(Error::ReadSubstrateStorageError)?
            .0;
        // we need to do some checks here:
        // 1. convert the public key to address and check it is not the same as the current maintainer.
        // 2. check if the nonce is greater than the current nonce.
        // 3. ~check if the signature is valid.~

        if new_maintainer == current_maintainer {
            tracing::warn!(
                current_maintainer =  %hex::encode(&current_maintainer),
                new_maintainer = %hex::encode(&new_maintainer),
                %nonce,
                signature = %hex::encode(&signature),
                "Skipping transfer ownership since the new maintainer is the same as the current one",
            );
            return Ok(());
        }
        let current_nonce =
            RuntimeApi::storage().signature_bridge().maintainer_nonce();

        let current_nonce = api
            .storage()
            .at(None)
            .await?
            .fetch(&current_nonce)
            .await?
            .expect("fetch current nonce from storage");

        if nonce <= current_nonce {
            tracing::warn!(
                %current_nonce,
                new_maintainer = %hex::encode(&new_maintainer),
                %nonce,
                signature = %hex::encode(&signature),
                "Skipping transfer ownership since the nonce is not greater than the current one",
            );
            return Ok(());
        }

        tracing::event!(
            target: webb_relayer_utils::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %webb_relayer_utils::probe::Kind::SignatureBridge,
            call = "transfer_ownership_with_signature_pub_key",
            chain_id = %chain_id,
            new_maintainer = %hex::encode(&new_maintainer),
            %nonce,
            signature = %hex::encode(&signature),
        );

        let mut message = nonce.to_be_bytes().to_vec();
        message.extend_from_slice(&new_maintainer);

        let set_maintainer_call = SetMaintainer {
            message: BoundedVec(message.clone()),
            signature: BoundedVec(signature.clone()),
        };

        // webb dynamic payload
        tracing::debug!("DKG Message Payload : {:?}", message);
        let set_maintainer_tx = WebbDynamicTxPayload {
            pallet_name: Cow::Borrowed("SignatureBridge"),
            call_name: Cow::Borrowed("set_maintainer"),
            fields: vec![
                Value::from_bytes(message),
                Value::from_bytes(signature),
            ],
        };

        let data_hash = utils::keccak256(set_maintainer_call.encode());
        let tx_key = SledQueueKey::from_substrate_with_custom_key(
            chain_id,
            make_execute_proposal_key(data_hash),
        );
        // Enqueue WebbDynamicTxPayload in protocol-substrate transaction queue
        QueueStore::<WebbDynamicTxPayload>::enqueue_item(
            &store,
            tx_key,
            set_maintainer_tx,
        )?;
        tracing::debug!(
            data_hash = ?hex::encode(data_hash),
            "Enqueued set-maintainer call for execution through protocol-substrate tx queue",
        );
        Ok(())
    }
}

pub fn validate_ecdsa_signature(
    data: &[u8],
    signature: &[u8],
    maintainer: &[u8],
) -> Result<bool, libsecp256k1::Error> {
    const SIGNATURE_LENGTH: usize = 65;
    if signature.len() == SIGNATURE_LENGTH {
        let mut sig = [0u8; SIGNATURE_LENGTH];
        sig[..SIGNATURE_LENGTH].copy_from_slice(signature);

        let hash = keccak_256(data);
        let pub_key = secp256k1_ecdsa_recover(&sig, &hash)?;
        Ok(pub_key == *maintainer)
    } else {
        Ok(false)
    }
}

fn secp256k1_ecdsa_recover(
    sig: &[u8; 65],
    msg: &[u8; 32],
) -> Result<[u8; 64], libsecp256k1::Error> {
    let rs = libsecp256k1::Signature::parse_standard_slice(&sig[0..64])
        .map_err(|_| libsecp256k1::Error::InvalidSignature)?;
    let v = libsecp256k1::RecoveryId::parse(if sig[64] > 26 {
        sig[64] - 27
    } else {
        sig[64]
    })
    .map_err(|_| libsecp256k1::Error::InvalidSignature)?;
    let pubkey =
        libsecp256k1::recover(&libsecp256k1::Message::parse(msg), &rs, &v)
            .map_err(|_| libsecp256k1::Error::InvalidSignature)?;
    let mut res = [0u8; 64];
    res.copy_from_slice(&pubkey.serialize()[1..65]);
    Ok(res)
}

fn make_execute_proposal_key(data_hash: [u8; 32]) -> [u8; 64] {
    let mut result = [0u8; 64];
    let prefix = b"execute_proposal_with_signature_";
    result[0..32].copy_from_slice(prefix);
    result[32..64].copy_from_slice(&data_hash);
    result
}
