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
use std::ops;
use std::sync::Arc;
use std::time::Duration;
use crate::context::RelayerContext;
use webb::evm::contract::protocol_solidity::{
    SignatureBridgeContract, SignatureBridgeContractEvents,
};
use webb::substrate::protocol_substrate_runtime::api::runtime_types::webb_standalone_runtime::Call;
use webb::evm::ethers::core::types::transaction::eip2718::TypedTransaction;
use webb::evm::ethers::prelude::*;
use webb::evm::ethers::providers;
use webb::evm::ethers::types;
use webb::evm::ethers::utils;
use ethereum_types::{U256, U64};
use crate::config;
use crate::events_watcher::{BridgeWatcher, EventWatcher};
use crate::store::sled::{SledQueueKey, SledStore};
use crate::store::{BridgeCommand, QueueStore};
use webb::substrate::{protocol_substrate_runtime, subxt::{self, DefaultConfig, PairSigner}};
type HttpProvider = providers::Provider<providers::Http>;
use super::{BlockNumberOf, SubstrateEventWatcher};
use crate::events_watcher::SubstrateBridgeWatcher;
use crate::store::LeafCacheStore;
use webb::substrate::protocol_substrate_runtime::api::anchor_bn254;

/// A SignatureBridge contract events & commands watcher.
#[derive(Copy, Clone, Debug, Default)]
pub struct SignatureBridgePalletWatcher;

#[async_trait::async_trait]
impl SubstrateEventWatcher for SignatureBridgePalletWatcher {
    const TAG: &'static str = "Substrate bridge pallet Watcher";

    type RuntimeConfig = subxt::DefaultConfig;

    type Api = protocol_substrate_runtime::api::RuntimeApi<
        Self::RuntimeConfig,
        subxt::DefaultExtra<Self::RuntimeConfig>,
    >;

    type Event = anchor_bn254::events::Deposit;

    type Store = SledStore;

    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        api: Arc<Self::Api>,
        (event, block_number): (Self::Event, BlockNumberOf<Self>),
    ) -> anyhow::Result<()> {
        // fetch chain_id
        let chain_id =
            api.constants().linkable_tree_bn254().chain_identifier()?;
        // fetch leaf_index from merkle tree at given block_number
        let at_hash = api
            .storage()
            .system()
            .block_hash(block_number, None)
            .await?;
        let next_leaf_index = api
            .storage()
            .merkle_tree_bn254()
            .next_leaf_index(event.tree_id, Some(at_hash))
            .await?;
        let leaf_index = next_leaf_index - 1;
        let chain_id = types::U256::from(chain_id);
        let tree_id = event.tree_id.to_string();
        let leaf = event.leaf;
        let value = (leaf_index, H256::from_slice(&leaf.0));
        store.insert_leaves((chain_id, tree_id.clone()), &[value])?;
        store.insert_last_deposit_block_number(
            (chain_id, tree_id.clone()),
            types::U64::from(block_number),
        )?;
        tracing::event!(
            target: crate::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %crate::probe::Kind::LeavesStore,
            chain_id = %chain_id,
            leaf_index = %leaf_index,
            leaf = %value.1,
            tree_id = %tree_id,
            block_number = %block_number
        );
        Ok(())
    }
}

#[async_trait::async_trait]
impl SubstrateBridgeWatcher for SignatureBridgePalletWatcher {
    #[tracing::instrument(skip_all)]
    async fn handle_cmd(
        &self,
        node_name: String,
        chain_id: U256,
        store: Arc<Self::Store>,
        ctx: &RelayerContext,
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
                    &ctx,
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

impl SignatureBridgePalletWatcher
where
    Self: SubstrateBridgeWatcher,
{
    #[tracing::instrument(skip_all)]
    async fn execute_proposal_with_signature(
        &self,
        node_name: String,
        chain_id: U256,
        store: Arc<<Self as SubstrateEventWatcher>::Store>,
        ctx: &RelayerContext,
        api: Arc<<Self as SubstrateEventWatcher>::Api>,
        (data, signature): (Vec<u8>, Vec<u8>),
    ) -> anyhow::Result<()> {
        // before doing anything, we need to do just two things:
        // 1. check if we already have this transaction in the queue.
        // 2. if not, check if the signature is valid.

        let data_hash = utils::keccak256(&data);
        let tx_key = SledQueueKey::from_evm_with_custom_key(
            chain_id,
            make_execute_proposal_key(data_hash),
        );
        let chain_id = chain_id.as_u32();

        // check if we already have a queued tx for this proposal.
        // if we do, we should not enqueue it again.
        let qq = QueueStore::<TypedTransaction>::has_item(&store, tx_key)?;
        if qq {
            tracing::debug!(
                data_hash = ?hex::encode(data_hash),
                "Skipping execution of the proposal since it is already in tx queue",
            );
            return Ok(());
        }

        // now we need to check if the signature is valid.
        let (data_clone, signature_clone) = (data.clone(), signature.clone());

        let data_hex = hex::encode(&data);
        let signature_hex = hex::encode(&signature);

        let parsed_proposal_bytes = parse_call_from_proposal_data(&data);
        let proposal_encoded_call: Call =
            scale::Decode::decode(&mut parsed_proposal_bytes.as_slice())
                .unwrap();

        tracing::event!(
            target: crate::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %crate::probe::Kind::SignatureBridge,
            call = "execute_proposal_with_signature",
            chain_id = %chain_id,
            data = ?data_hex,
            signature = ?signature_hex,
            data_hash = ?hex::encode(data_hash),
        );
        // I guess now we are ready to enqueue the transaction.
        let execute_proposal_tx = api.tx().signature_bridge().execute_proposal(
            chain_id.into(),
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
        //todo!
        // QueueStore::<TypedTransaction>::enqueue_item(&store, tx_key, execute_proposal_tx.tx)?;
        // tracing::debug!(
        //     data_hash = ?hex::encode(data_hash),
        //     "Enqueued the proposal for execution in the tx queue",
        // );
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
