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
use tangle_subxt::subxt::{self, OnlineClient};use tangle_subxt::tangle_testnet_runtime::api::jobs::events::JobResultSubmitted;
use tangle_subxt::tangle_testnet_runtime::api as RuntimeApi;
use tangle_subxt::tangle_testnet_runtime::api::runtime_types::tangle_primitives::jobs::JobResult;
use tangle_subxt::tangle_testnet_runtime::api::runtime_types::tangle_primitives::roles::RoleType;
use webb_proposals::evm::AnchorUpdateProposal;
use webb_relayer_store::queue::{QueueItem, QueueStore};
use webb_relayer_store::sled::{SledQueueKey, SledStore};
use webb_relayer_store::{BridgeCommand, BridgeKey};
use webb_relayer_utils::{metric, TangleRuntimeConfig};

use webb_event_watcher_traits::substrate::EventHandler;

/// JobResultHandler  handles the `JobResultSubmitted` event.
#[derive(Clone, Debug)]
pub struct JobResultHandler {
    relayer_config: webb_relayer_config::WebbRelayerConfig,
}

impl JobResultHandler {
    pub fn new(relayer_config: webb_relayer_config::WebbRelayerConfig) -> Self {
        Self { relayer_config }
    }
}

#[async_trait::async_trait]
impl EventHandler<TangleRuntimeConfig> for JobResultHandler {
    type Client = OnlineClient<TangleRuntimeConfig>;

    type Store = SledStore;

    async fn can_handle_events(
        &self,
        events: subxt::events::Events<TangleRuntimeConfig>,
    ) -> webb_relayer_utils::Result<bool> {
        let has_event = events.find::<JobResultSubmitted>().any(|event| {
            matches!(
                event,
                Ok(JobResultSubmitted {
                    role_type: RoleType::Tss(_),
                    ..
                })
            )
        });

        Ok(has_event)
    }

    async fn handle_events(
        &self,
        store: Arc<Self::Store>,
        client: Arc<Self::Client>,
        (events, _block_number): (
            subxt::events::Events<TangleRuntimeConfig>,
            u64,
        ),
        metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()> {
        // We go the job result submitted event
        let job_result_submitted_events: Vec<_> = events
            .find::<JobResultSubmitted>()
            .flatten()
            .filter(|event| {
                matches!(
                    event,
                    JobResultSubmitted {
                        role_type: RoleType::Tss(_),
                        ..
                    }
                )
            })
            .collect();
        for event in job_result_submitted_events {
            // Fetch submitted job result
            let job_id = event.clone().job_id;
            let known_result_addrs = RuntimeApi::storage()
                .jobs()
                .known_results(event.clone().role_type, job_id.clone());

            let maybe_result = client
                .storage()
                .at_latest()
                .await?
                .fetch(&known_result_addrs)
                .await?;

            if let Some(phase_result) = maybe_result {
                match phase_result.result {
                    JobResult::DKGPhaseTwo(result) => {
                        let anchor_update_proposal =
                            webb_proposals::from_slice::<AnchorUpdateProposal>(
                                &result.data.0,
                            )?;
                        let destination_resource_id =
                            anchor_update_proposal.header().resource_id();
                        let bridge_key = BridgeKey::new(
                            destination_resource_id.typed_chain_id(),
                        );

                        metrics.lock().await.proposals_signed.inc();

                        let signature = if result.signature.0.len() == 64 {
                            let mut sig = result.signature.0.clone();
                            sig.push(28);
                            sig
                        } else {
                            result.signature.0.clone()
                        };
                        tracing::debug!(
                            %bridge_key,
                            proposal = ?anchor_update_proposal,
                            signature = ?signature,
                            "Signaling Signature Bridge to execute proposal",
                        );
                        let item = QueueItem::new(
                            BridgeCommand::ExecuteProposalWithSignature {
                                data: result.data.0,
                                signature,
                            },
                        );
                        store.enqueue_item(
                            SledQueueKey::from_bridge_key(bridge_key),
                            item,
                        )?;
                    }

                    JobResult::DKGPhaseFour(result) => {
                        tracing::debug!("DKG Phase Four result received");
                        let mut bridge_keys = Vec::new();
                        for (_, config) in self.relayer_config.evm.iter() {
                            let typed_chain_id =
                                webb_proposals::TypedChainId::Evm(
                                    config.chain_id,
                                );
                            let bridge_key = BridgeKey::new(typed_chain_id);
                            bridge_keys.push(bridge_key);
                        }

                        // Now we just signal every signature bridge to transfer the ownership.
                        for bridge_key in bridge_keys {
                            tracing::debug!(
                                %bridge_key,
                                ?event,
                                "Signaling Signature Bridge to transfer ownership",
                            );
                            tracing::event!(
                                target: webb_relayer_utils::probe::TARGET,
                                tracing::Level::DEBUG,
                                kind = %webb_relayer_utils::probe::Kind::SigningBackend,
                                backend = "DKG",
                                signal_bridge = %bridge_key,
                                public_key = %hex::encode(&result.key.clone().0),
                                previoud_job_id = %result.phase_one_id,
                                new_job_id = %result.new_phase_one_id,
                                signature = %hex::encode(&result.signature.clone().0),
                            );

                            let item = QueueItem::new(
                                BridgeCommand::TransferOwnership {
                                    job_id: result.new_phase_one_id,
                                    pub_key: result.key.clone().0,
                                    signature: result.signature.clone().0,
                                },
                            );

                            store.enqueue_item(
                                SledQueueKey::from_bridge_key(bridge_key),
                                item,
                            )?;
                        }
                    }
                    _ => unimplemented!("Phase results not supported"),
                }
            }
        }

        Ok(())
    }
}
