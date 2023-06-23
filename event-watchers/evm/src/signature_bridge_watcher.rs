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

use std::ops;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Mutex;
use webb::evm::contract::protocol_solidity::signature_bridge::{
    AdminSetResourceWithSignatureCall, SignatureBridgeContract,
    SignatureBridgeContractEvents,
};
use webb::evm::ethers::contract::Contract;
use webb::evm::ethers::core::types::transaction::eip2718::TypedTransaction;
use webb::evm::ethers::prelude::*;
use webb::evm::ethers::types;
use webb::evm::ethers::utils;

use webb_event_watcher_traits::evm::{
    BridgeWatcher, EventHandler, EventWatcher, WatchableContract,
};
use webb_event_watcher_traits::EthersTimeLagClient;
use webb_relayer_store::sled::{SledQueueKey, SledStore};
use webb_relayer_store::{BridgeCommand, QueueStore};
use webb_relayer_utils::metric;

/// A Wrapper around the `SignatureBridgeContract` contract.
#[derive(Debug)]
pub struct SignatureBridgeContractWrapper<M: Middleware> {
    config: webb_relayer_config::evm::SignatureBridgeContractConfig,
    contract: Arc<SignatureBridgeContract<M>>,
}

impl<M: Middleware> Clone for SignatureBridgeContractWrapper<M> {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            contract: Arc::clone(&self.contract),
        }
    }
}

impl<M: Middleware> SignatureBridgeContractWrapper<M> {
    pub fn new(
        config: webb_relayer_config::evm::SignatureBridgeContractConfig,
        client: Arc<M>,
    ) -> Self {
        Self {
            contract: Arc::new(SignatureBridgeContract::new(
                config.common.address,
                client,
            )),
            config,
        }
    }
}

impl<M: Middleware> ops::Deref for SignatureBridgeContractWrapper<M> {
    type Target = Contract<M>;

    fn deref(&self) -> &Self::Target {
        &self.contract
    }
}

impl<M: Middleware> WatchableContract for SignatureBridgeContractWrapper<M> {
    fn deployed_at(&self) -> types::U64 {
        self.config.common.deployed_at.into()
    }

    fn polling_interval(&self) -> Duration {
        Duration::from_millis(self.config.events_watcher.polling_interval)
    }

    fn max_blocks_per_step(&self) -> types::U64 {
        self.config.events_watcher.max_blocks_per_step.into()
    }

    fn print_progress_interval(&self) -> Duration {
        Duration::from_millis(
            self.config.events_watcher.print_progress_interval,
        )
    }
}

/// A SignatureBridge contract events & commands watcher.
#[derive(Copy, Clone, Debug, Default)]
pub struct SignatureBridgeContractWatcher;

#[derive(Copy, Clone, Debug, Default)]
pub struct SignatureBridgeGovernanceOwnershipTransferredHandler;

#[async_trait::async_trait]
impl EventWatcher for SignatureBridgeContractWatcher {
    const TAG: &'static str = "Signature Bridge Watcher";

    type Contract = SignatureBridgeContractWrapper<EthersTimeLagClient>;

    type Events = SignatureBridgeContractEvents;

    type Store = SledStore;
}

#[async_trait::async_trait]
impl EventHandler for SignatureBridgeGovernanceOwnershipTransferredHandler {
    type Contract = SignatureBridgeContractWrapper<EthersTimeLagClient>;

    type Events = SignatureBridgeContractEvents;

    type Store = SledStore;

    async fn can_handle_events(
        &self,
        (events, _meta): (Self::Events, LogMeta),
        _wrapper: &Self::Contract,
    ) -> webb_relayer_utils::Result<bool> {
        use SignatureBridgeContractEvents::*;
        let has_event =
            matches!(events, GovernanceOwnershipTransferredFilter(_));
        Ok(has_event)
    }

    #[tracing::instrument(
        skip_all,
        fields(event_type = ?e.0),
    )]
    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        wrapper: &Self::Contract,
        e: (Self::Events, LogMeta),
        _metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()> {
        let event = e.0;
        match event {
            SignatureBridgeContractEvents::GovernanceOwnershipTransferredFilter(v) => {
                // if the ownership is transferred to the new owner, we need to
                // to check our txqueue and remove any pending tx that was trying to
                // do this transfer.
                let chain_id = wrapper.contract.get_chain_id().call().await?;
                let tx_key = SledQueueKey::from_evm_with_custom_key(
                    chain_id.as_u32(),
                    make_transfer_ownership_key(v.new_owner.to_fixed_bytes())
                );
                let exist_tx = QueueStore::<TypedTransaction>::has_item(&store, tx_key)?;
                if !exist_tx {
                    return Ok(());
                }
                let result = QueueStore::<TypedTransaction>::remove_item(&store, tx_key);
                if result.is_ok() {
                    tracing::debug!("Removed pending transfer ownership tx from txqueue")
                }
            },
            e => tracing::debug!("Got Event {:?}", e),
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl BridgeWatcher for SignatureBridgeContractWatcher {
    #[tracing::instrument(skip_all)]
    async fn handle_cmd(
        &self,
        store: Arc<Self::Store>,
        wrapper: &Self::Contract,
        cmd: BridgeCommand,
    ) -> webb_relayer_utils::Result<()> {
        use BridgeCommand::*;
        tracing::trace!("Got cmd {:?}", cmd);
        match cmd {
            ExecuteProposalWithSignature { data, signature } => {
                self.execute_proposal_with_signature(
                    store,
                    &wrapper.contract,
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
                    store,
                    &wrapper.contract,
                    (public_key, nonce, signature),
                )
                .await?
            }
            AdminSetResourceWithSignature {
                resource_id,
                new_resource_id,
                handler_address,
                nonce,
                signature,
            } => {
                self.admin_set_resource_with_signature(
                    store,
                    &wrapper.contract,
                    (
                        resource_id,
                        new_resource_id,
                        handler_address,
                        nonce,
                        signature,
                    ),
                )
                .await?
            }
        };
        Ok(())
    }
}

impl SignatureBridgeContractWatcher
where
    Self: BridgeWatcher,
{
    #[tracing::instrument(skip_all)]
    async fn execute_proposal_with_signature(
        &self,
        store: Arc<<Self as EventWatcher>::Store>,
        contract: &SignatureBridgeContract<EthersTimeLagClient>,
        (proposal_data, signature): (Vec<u8>, Vec<u8>),
    ) -> webb_relayer_utils::Result<()> {
        let proposal_data_hex = hex::encode(&proposal_data);
        // 1. Verify proposal length. Proposal lenght should be greater than 40 bytes (proposal header(40B) + proposal body).
        if proposal_data.len() < 40 {
            tracing::warn!(
                proposal_data = %proposal_data_hex,
                "Skipping execution of this proposal: Invalid Proposal",
            );
            return Ok(());
        }

        // 2. Verify if proposal already exists in transaction queue
        let chain_id = contract.get_chain_id().call().await?;
        let proposal_data_hash = utils::keccak256(&proposal_data);
        let tx_key = SledQueueKey::from_evm_with_custom_key(
            chain_id.as_u32(),
            make_execute_proposal_key(proposal_data_hash),
        );

        // check if we already have a queued tx for this proposal.
        // if we do, we should not enqueue it again.
        let qq = QueueStore::<TypedTransaction>::has_item(&store, tx_key)?;
        if qq {
            tracing::debug!(
                proposal_data_hash = %hex::encode(proposal_data_hash),
                "Skipping execution of this proposal: Already Exists in Queue",
            );
            return Ok(());
        }

        // 3. Verify proposal signature. Proposal should be signed by active maintainer/dkg-key
        let (proposal_data_clone, signature_clone) =
            (proposal_data.clone(), signature.clone());
        let is_signature_valid = contract
            .is_signature_from_governor(
                proposal_data_clone.into(),
                signature_clone.into(),
            )
            .call()
            .await?;

        let governor = contract.governor().call().await?;
        tracing::debug!(
            governor = %hex::encode(governor),
            "GOVERNOR",
        );
        let signature_hex = hex::encode(&signature);
        if !is_signature_valid {
            tracing::warn!(
                proposal_data = %proposal_data_hex,
                signature = %signature_hex,
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
            chain_id = %chain_id.as_u64(),
            proposal_data = %proposal_data_hex,
            signature = %signature_hex,
            proposal_data_hash = %hex::encode(proposal_data_hash),
        );
        // Enqueue transaction call data in evm transaction queue
        let call = contract.execute_proposal_with_signature(
            proposal_data.into(),
            signature.into(),
        );
        QueueStore::<TypedTransaction>::enqueue_item(&store, tx_key, call.tx)?;
        tracing::debug!(
            proposal_data_hash = ?hex::encode(proposal_data_hash),
            "Enqueued execute-proposal call for execution through evm tx queue",
        );
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn transfer_ownership_with_signature(
        &self,
        store: Arc<<Self as EventWatcher>::Store>,
        contract: &SignatureBridgeContract<EthersTimeLagClient>,
        (public_key, nonce, signature): (Vec<u8>, u32, Vec<u8>),
    ) -> webb_relayer_utils::Result<()> {
        // before doing anything, we need to do just two things:
        // 1. check if we already have this transaction in the queue.
        // 2. if not, check if the signature is valid.

        let chain_id = contract.get_chain_id().call().await?;
        let new_governor_address =
            eth_address_from_uncompressed_public_key(&public_key);
        let tx_key = SledQueueKey::from_evm_with_custom_key(
            chain_id.as_u32(),
            make_transfer_ownership_key(new_governor_address.to_fixed_bytes()),
        );

        // check if we already have a queued tx for this action.
        // if we do, we should not enqueue it again.
        let qq = QueueStore::<TypedTransaction>::has_item(&store, tx_key)?;
        if qq {
            tracing::debug!(
                "Skipping transfer ownership since it is already in tx queue",
            );
            return Ok(());
        }
        // we need to do some checks here:
        // 1. convert the public key to address and check it is not the same as the current governor.
        // 2. check if the nonce is greater than the current nonce.
        // 3. ~check if the signature is valid.~
        let current_governor_address = contract.governor().call().await?;
        if new_governor_address == current_governor_address {
            tracing::warn!(
                %new_governor_address,
                %current_governor_address,
                public_key = %hex::encode(&public_key),
                %nonce,
                signature = %hex::encode(&signature),
                "Skipping transfer ownership since the new governor is the same as the current one",
            );
            return Ok(());
        }

        let refresh_nonce = contract.refresh_nonce().call().await?;

        // require(refreshNonce < nonce, "Invalid nonce")
        if nonce < refresh_nonce {
            tracing::warn!(
                %refresh_nonce,
                %nonce,
                public_key = %hex::encode(&public_key),
                signature = %hex::encode(&signature),
                "Skipping transfer ownership since the nonce is less than the refresh nonce",
            );
            return Ok(());
        }
        // require(nonce <= refreshNonce + 1, "Nonce must increment by 1");
        if nonce != refresh_nonce + 1 {
            tracing::warn!(
                %refresh_nonce,
                %nonce,
                public_key = %hex::encode(&public_key),
                signature = %hex::encode(&signature),
                "Skipping transfer ownership since the nonce must increment by 1",
            );
            return Ok(());
        }
        tracing::event!(
            target: webb_relayer_utils::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %webb_relayer_utils::probe::Kind::SignatureBridge,
            call = "transfer_ownership_with_signature_pub_key",
            chain_id = %chain_id.as_u64(),
            public_key = %hex::encode(&public_key),
            %nonce,
            signature = %hex::encode(&signature),
        );
        // estimated gas
        let estimate_gas = contract
            .transfer_ownership_with_signature_pub_key(
                public_key.clone().into(),
                nonce,
                signature.clone().into(),
            )
            .estimate_gas()
            .await?;

        // get the current governor nonce.
        let call = contract
            .transfer_ownership_with_signature_pub_key(
                public_key.into(),
                nonce,
                signature.into(),
            )
            .gas(estimate_gas.saturating_mul(U256::from(2)));

        QueueStore::<TypedTransaction>::enqueue_item(&store, tx_key, call.tx)?;
        tracing::debug!(
            chain_id = %chain_id.as_u64(),
            "Enqueued the ownership transfer for execution in the tx queue",
        );
        Ok(())
    }

    #[tracing::instrument(skip_all)]
    async fn admin_set_resource_with_signature(
        &self,
        store: Arc<<Self as EventWatcher>::Store>,
        contract: &SignatureBridgeContract<EthersTimeLagClient>,
        (resource_id, new_resource_id, handler_address, nonce, signature): (
            [u8; 32],
            [u8; 32],
            [u8; 20],
            u32,
            Vec<u8>,
        ),
    ) -> webb_relayer_utils::Result<()> {
        let function_sig = AdminSetResourceWithSignatureCall::selector();
        let mut proposal_data = Vec::with_capacity(32 + 32 + 20);
        proposal_data.extend_from_slice(resource_id.as_slice());
        proposal_data.extend_from_slice(function_sig.as_slice());
        proposal_data.extend_from_slice(&nonce.to_be_bytes());
        proposal_data.extend_from_slice(new_resource_id.as_slice());
        proposal_data.extend_from_slice(handler_address.as_slice());
        let proposal_data_hash = utils::keccak256(&proposal_data);

        let chain_id = contract.get_chain_id().call().await?;

        let tx_key = SledQueueKey::from_evm_with_custom_key(
            chain_id.as_u32(),
            make_admin_set_resource_key(proposal_data_hash),
        );
        // check if we already have a queued tx for this proposal.
        // if we do, we should not enqueue it again.
        let qq = QueueStore::<TypedTransaction>::has_item(&store, tx_key)?;
        if qq {
            tracing::debug!(
                proposal_data_hash = %hex::encode(proposal_data_hash),
                "Skipping execution of this proposal: Already Exists in Queue",
            );
            return Ok(());
        }

        // Verify proposal signature. Proposal should be signed by active maintainer/dkg-key
        let (proposal_data_clone, signature_clone) =
            (proposal_data.clone(), signature.clone());
        let is_signature_valid = contract
            .is_signature_from_governor(
                proposal_data_clone.into(),
                signature_clone.into(),
            )
            .call()
            .await?;

        let governor = contract.governor().call().await?;
        tracing::debug!(
            governor = %hex::encode(governor),
            "GOVERNOR",
        );
        let proposal_data_hex = hex::encode(&proposal_data);
        let signature_hex = hex::encode(&signature);
        if !is_signature_valid {
            tracing::warn!(
                proposal_data = %proposal_data_hex,
                signature = %signature_hex,
                "Skipping execution of this proposal: Invalid Signature",
            );
            return Ok(());
        }

        // 3. Enqueue proposal for execution.
        tracing::event!(
            target: webb_relayer_utils::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %webb_relayer_utils::probe::Kind::SignatureBridge,
            call = "admin_set_resource_with_signature",
            chain_id = %chain_id.as_u64(),
            proposal_data = %proposal_data_hex,
            signature = %signature_hex,
            proposal_data_hash = %hex::encode(proposal_data_hash),
        );
        // Enqueue transaction call data in evm transaction queue
        let call = contract.admin_set_resource_with_signature(
            resource_id,
            function_sig,
            nonce,
            new_resource_id,
            handler_address.into(),
            signature.into(),
        );
        QueueStore::<TypedTransaction>::enqueue_item(&store, tx_key, call.tx)?;
        tracing::debug!(
            proposal_data_hash = %hex::encode(proposal_data_hash),
            "Enqueued admin_set_resource_with_signature call for execution through evm tx queue",
        );
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

fn make_transfer_ownership_key(new_owner_address: [u8; 20]) -> [u8; 64] {
    let mut result = [0u8; 64];
    let prefix = b"transfer_ownership_with_sig_key_";
    result[0..32].copy_from_slice(prefix);
    result[32..52].copy_from_slice(&new_owner_address);
    result
}

fn make_admin_set_resource_key(data_hash: [u8; 32]) -> [u8; 64] {
    let mut result = [0u8; 64];
    let prefix = b"admin_set_resource_with_sig_key_";
    result[0..32].copy_from_slice(prefix);
    result[32..64].copy_from_slice(&data_hash);
    result
}

/// Get the Ethereum address from the uncompressed EcDSA public key.
fn eth_address_from_uncompressed_public_key(pub_key: &[u8]) -> Address {
    // hash the public key.
    let pub_key_hash = utils::keccak256(pub_key);
    // take the last 20 bytes of the hash.
    let mut address_bytes = [0u8; 20];
    address_bytes.copy_from_slice(&pub_key_hash[12..32]);
    Address::from(address_bytes)
}

#[cfg(test)]
mod tests {
    use crate::signature_bridge_watcher::eth_address_from_uncompressed_public_key;

    #[test]
    fn should_get_the_correct_eth_address_from_public_key() {
        // given
        let public_key_uncompressed_hex = hex::decode(
            "58eb6e2a1901baead22a6f021454638c2abeb8e179400879098f327cebdecf44823ec88239a198b00622768b8461e66c7531a6b22be417db0069be28abe1bdf3",
        ).unwrap();
        // when
        let address = eth_address_from_uncompressed_public_key(
            &public_key_uncompressed_hex,
        );
        // then
        assert_eq!(
            address,
            "0x9dD0de7Ff10D3eB77F0488039591498f32a23c8A"
                .parse()
                .unwrap()
        );
    }
}
