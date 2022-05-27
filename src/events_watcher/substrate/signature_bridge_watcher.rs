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
use webb::substrate::subxt::sp_core::ecdsa::Signature;
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
        store: Arc<Self::Store>,
        api: Arc<Self::Api>,
        (event, block_number): (Self::Event, BlockNumberOf<Self>),
    ) -> anyhow::Result<()> {
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
        //    let api: Arc<Self::Api> = Arc::new(client_api.to_runtime_api());
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
        store: Arc<<Self as SubstrateEventWatcher>::Store>,
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
        store: Arc<<Self as SubstrateEventWatcher>::Store>,
        ctx: &RelayerContext,
        api: Arc<<Self as SubstrateEventWatcher>::Api>,
        (public_key, nonce, signature): (Vec<u8>, u32, Vec<u8>),
    ) -> anyhow::Result<()> {
        let new_maintainer = public_key.clone();
        let current_maintainer =
            api.storage().signature_bridge().maintainer(None).await?;
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

fn make_execute_proposal_key(data_hash: [u8; 32]) -> [u8; 64] {
    let mut result = [0u8; 64];
    let prefix = b"execute_proposal_with_signature_";
    result[0..32].copy_from_slice(prefix);
    result[32..64].copy_from_slice(&data_hash);
    result
}

pub fn parse_call_from_proposal_data(proposal_data: &Vec<u8>) -> Vec<u8> {
    // Not [36..] because there are 4 byte of zero padding to match Solidity side
    proposal_data[40..].to_vec()
}

pub fn validate_ecdsa_signature(data: &[u8], signature: &[u8]) -> bool {
    const SIGNATURE_LENGTH: usize = 65;
    if signature.len() == SIGNATURE_LENGTH {
        let mut sig = [0u8; SIGNATURE_LENGTH];
        sig[..SIGNATURE_LENGTH].copy_from_slice(&signature);
        let signature: Signature = Signature::from_raw(sig);
        return signature.recover(data).is_some();
    } else {
        return false;
    }
}
