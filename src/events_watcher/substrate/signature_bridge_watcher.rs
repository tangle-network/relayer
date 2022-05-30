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
//

use std::sync::Arc;
use crate::context::RelayerContext;
use webb::substrate::subxt::sp_core::hashing::keccak_256;
use webb::substrate::protocol_substrate_runtime::api::runtime_types::webb_standalone_runtime::Call;
use super::{BlockNumberOf, SubstrateEventWatcher};
use crate::events_watcher::SubstrateBridgeWatcher;
use crate::store::sled::SledStore;
use crate::store::BridgeCommand;
use ethereum_types::U256;
use futures::StreamExt;
use webb::substrate::{
    protocol_substrate_runtime,
    subxt::{self, PairSigner},
};
use webb::substrate::protocol_substrate_runtime::api::signature_bridge;

/// A SignatureBridge contract events & commands watcher.
#[derive(Copy, Clone, Debug, Default)]
pub struct SubstrateBridgeEventWatcher;

#[async_trait::async_trait]
impl SubstrateEventWatcher for SubstrateBridgeEventWatcher {
    const TAG: &'static str = "Substrate bridge pallet Watcher";

    type RuntimeConfig = subxt::DefaultConfig;

    type Api = protocol_substrate_runtime::api::RuntimeApi<
        Self::RuntimeConfig,
        subxt::DefaultExtra<Self::RuntimeConfig>,
    >;

    type Event = signature_bridge::events::MaintainerSet;

    type Store = SledStore;

    async fn handle_event(
        &self,
        _store: Arc<Self::Store>,
        _api: Arc<Self::Api>,
        (event, _block_number): (Self::Event, BlockNumberOf<Self>),
    ) -> anyhow::Result<()> {
        // todo
        // if the ownership is transferred to the new owner, we need to
        // to check our txqueue and remove any pending tx that was trying to
        // do this transfer.
        tracing::event!(
            target: crate::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %crate::probe::Kind::SignatureBridge,
            call = "signature-bridge-maintainer-event",
            msg = "Maintainer set",
            new_maintainer = ?event.new_maintainer,
            old_maintainer = ?event.old_maintainer,
        );

        // mark this event as processed.
        // let events_bytes = &event.encode();
        // store.store_event(events_bytes)?;

        Ok(())
    }
}

#[async_trait::async_trait]
impl SubstrateBridgeWatcher for SubstrateBridgeEventWatcher {
    #[tracing::instrument(skip_all)]
    async fn handle_cmd(
        &self,
        node_name: String,
        chain_id: U256,
        store: Arc<Self::Store>,
        ctx: RelayerContext,
        api: Arc<Self::Api>,
        cmd: BridgeCommand,
    ) -> anyhow::Result<()> {
        use BridgeCommand::*;
        tracing::trace!("Got cmd {:?}", cmd);
        match cmd {
            ExecuteProposalWithSignature { data, signature } => {
                self.execute_proposal_with_signature(
                    node_name,
                    chain_id,
                    store,
                    ctx,
                    api.clone(),
                    (data, signature),
                )
                .await?;
            }
            TransferOwnershipWithSignature {
                public_key,
                nonce,
                signature,
            } => {
                self.transfer_ownership_with_signature(
                    node_name,
                    chain_id,
                    store,
                    &ctx,
                    api.clone(),
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
    Self: SubstrateBridgeWatcher,
{
    #[tracing::instrument(skip_all)]
    async fn execute_proposal_with_signature(
        &self,
        node_name: String,
        chain_id: U256,
        _store: Arc<<Self as SubstrateEventWatcher>::Store>,
        ctx: RelayerContext,
        api: Arc<<Self as SubstrateEventWatcher>::Api>,
        (data, signature): (Vec<u8>, Vec<u8>),
    ) -> anyhow::Result<()> {
        let data_hex = hex::encode(&data);
        let signature_hex = hex::encode(&signature);
        // parse proposal call
        let parsed_proposal_bytes = parse_call_from_proposal_data(&data);
        let proposal_encoded_call: Call =
            scale::Decode::decode(&mut parsed_proposal_bytes.as_slice())
                .unwrap();
        // now we need to check if the signature is valid.

        let is_signature_valid =
            validate_ecdsa_signature(data.as_slice(), signature.as_slice());

        if !is_signature_valid {
            tracing::warn!(
                data = ?data_hex,
                signature = ?signature_hex,
                "Skipping execution of this proposal since signature is invalid",
            );
            return Ok(());
        }

        tracing::event!(
            target: crate::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %crate::probe::Kind::SignatureBridge,
            call = "execute_proposal_with_signature",
            chain_id = %chain_id,
            data = ?data_hex,
            signature = ?signature_hex,
        );
        // todo! transaction queue
        let execute_proposal_tx = api.tx().signature_bridge().execute_proposal(
            chain_id.as_u64(),
            proposal_encoded_call,
            data,
            signature,
        );
        let pair = ctx.substrate_wallet(&node_name).await?;
        let signer = PairSigner::new(pair);
        let mut progress = execute_proposal_tx
            .sign_and_submit_then_watch(&signer)
            .await?;
        while let Some(event) = progress.next().await {
            let e = match event {
                Ok(e) => e,
                Err(err) => {
                    tracing::error!(error = %err, "failed to watch for tx events");
                    return Err(err.into());
                }
            };

            match e {
                subxt::TransactionStatus::Future => {}
                subxt::TransactionStatus::Ready => {
                    tracing::trace!("tx ready");
                }
                subxt::TransactionStatus::Broadcast(_) => {}
                subxt::TransactionStatus::InBlock(_) => {
                    tracing::trace!("tx in block");
                }
                subxt::TransactionStatus::Retracted(_) => {
                    tracing::warn!("tx retracted");
                }
                subxt::TransactionStatus::FinalityTimeout(_) => {
                    tracing::warn!("tx timeout");
                }
                subxt::TransactionStatus::Finalized(v) => {
                    let maybe_success = v.wait_for_success().await;
                    match maybe_success {
                        Ok(events) => {
                            tracing::debug!(?events, "tx finalized",);
                        }
                        Err(err) => {
                            tracing::error!(error = %err, "tx failed");
                            return Err(err.into());
                        }
                    }
                }
                subxt::TransactionStatus::Usurped(_) => {}
                subxt::TransactionStatus::Dropped => {
                    tracing::warn!("tx dropped");
                }
                subxt::TransactionStatus::Invalid => {
                    tracing::warn!("tx invalid");
                }
            }
        }

        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn transfer_ownership_with_signature(
        &self,
        node_name: String,
        chain_id: U256,
        _store: Arc<<Self as SubstrateEventWatcher>::Store>,
        ctx: &RelayerContext,
        api: Arc<<Self as SubstrateEventWatcher>::Api>,
        (public_key, nonce, signature): (Vec<u8>, u32, Vec<u8>),
    ) -> anyhow::Result<()> {
        let new_maintainer = public_key.clone();
        // get current maintainer
        let current_maintainer =
            api.storage().signature_bridge().maintainer(None).await?;

        // we need to do some checks here:
        // 1. convert the public key to address and check it is not the same as the current governor.
        // 2. check if the nonce is greater than the current nonce.
        // 3. ~check if the signature is valid.~

        if new_maintainer == current_maintainer {
            tracing::warn!(
                current_mainatiner =  %hex::encode(&current_maintainer),
                new_maintainer = %hex::encode(&new_maintainer),
                %nonce,
                signature = %hex::encode(&signature),
                "Skipping transfer ownership since the new governor is the same as the current one",
            );
            return Ok(());
        }
        let current_nonce = api
            .storage()
            .signature_bridge()
            .maintainer_nonce(None)
            .await?;
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
            target: crate::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %crate::probe::Kind::SignatureBridge,
            call = "transfer_ownership_with_signature_pub_key",
            chain_id = %chain_id.as_u64(),
            new_maintainer = %hex::encode(&new_maintainer),
            %nonce,
            signature = %hex::encode(&signature),
        );

        let set_maintainer_tx = api
            .tx()
            .signature_bridge()
            .set_maintainer(new_maintainer, signature);

        let pair = ctx.substrate_wallet(&node_name).await?;
        let signer = PairSigner::new(pair);
        let mut progress = set_maintainer_tx
            .sign_and_submit_then_watch(&signer)
            .await?;
        while let Some(event) = progress.next().await {
            let e = match event {
                Ok(e) => e,
                Err(err) => {
                    tracing::error!(error = %err, "failed to watch for tx events");
                    return Err(err.into());
                }
            };

            match e {
                subxt::TransactionStatus::Future => {}
                subxt::TransactionStatus::Ready => {
                    tracing::trace!("tx ready");
                }
                subxt::TransactionStatus::Broadcast(_) => {}
                subxt::TransactionStatus::InBlock(_) => {
                    tracing::trace!("tx in block");
                }
                subxt::TransactionStatus::Retracted(_) => {
                    tracing::warn!("tx retracted");
                }
                subxt::TransactionStatus::FinalityTimeout(_) => {
                    tracing::warn!("tx timeout");
                }
                subxt::TransactionStatus::Finalized(v) => {
                    let maybe_success = v.wait_for_success().await;
                    match maybe_success {
                        Ok(events) => {
                            tracing::debug!(?events, "tx finalized",);
                        }
                        Err(err) => {
                            tracing::error!(error = %err, "tx failed");
                            return Err(err.into());
                        }
                    }
                }
                subxt::TransactionStatus::Usurped(_) => {}
                subxt::TransactionStatus::Dropped => {
                    tracing::warn!("tx dropped");
                }
                subxt::TransactionStatus::Invalid => {
                    tracing::warn!("tx invalid");
                }
            }
        }

        Ok(())
    }
}

pub fn parse_call_from_proposal_data(proposal_data: &[u8]) -> Vec<u8> {
    // Not [36..] because there are 4 byte of zero padding to match Solidity side
    proposal_data[40..].to_vec()
}

pub fn validate_ecdsa_signature(data: &[u8], signature: &[u8]) -> bool {
    const SIGNATURE_LENGTH: usize = 65;
    if signature.len() == SIGNATURE_LENGTH {
        let mut sig = [0u8; SIGNATURE_LENGTH];
        sig[..SIGNATURE_LENGTH].copy_from_slice(signature);

        let hash = keccak_256(data);

        secp256k1_ecdsa_recover(&sig, &hash).is_ok()
    } else {
        false
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
    } as u8)
    .map_err(|_| libsecp256k1::Error::InvalidSignature)?;
    let pubkey =
        libsecp256k1::recover(&libsecp256k1::Message::parse(msg), &rs, &v)
            .map_err(|_| libsecp256k1::Error::InvalidSignature)?;
    let mut res = [0u8; 64];
    res.copy_from_slice(&pubkey.serialize()[1..65]);
    Ok(res)
}
