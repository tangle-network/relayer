// Copyright (C) 2022-2024 Webb Technologies Inc.
//
// Tangle is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Tangle is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should receive a copy of the GNU General Public License
// If not, see <http://www.gnu.org/licenses/>.

use std::sync::Arc;
use tokio::sync::Mutex;
use webb::substrate::subxt::{self, OnlineClient};use webb::substrate::tangle_runtime::api::jobs::events::JobResultSubmitted;
use webb::substrate::tangle_runtime::api as RuntimeApi;
use webb::substrate::tangle_runtime::api::runtime_types::tangle_primitives::jobs::JobResult;
use webb::substrate::tangle_runtime::api::runtime_types::tangle_primitives::roles::RoleType;
use webb_relayer_store::sled::SledStore;
use webb_relayer_store::BridgeKey;
use webb_relayer_utils::{metric, TangleRuntimeConfig};

use webb_event_watcher_traits::substrate::EventHandler;

/// JobResultHandler  handles the `JobResultSubmitted` event and then signals
/// signature bridge watcher to update governor.
#[derive(Clone, Debug)]
pub struct JobResultHandler {
    webb_config: webb_relayer_config::WebbRelayerConfig,
}

impl JobResultHandler {
    pub fn new(webb_config: webb_relayer_config::WebbRelayerConfig) -> Self {
        Self { webb_config }
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
        _store: Arc<Self::Store>,
        client: Arc<Self::Client>,
        (events, _block_number): (
            subxt::events::Events<TangleRuntimeConfig>,
            u64,
        ),
        _metrics: Arc<Mutex<metric::Metrics>>,
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
            // Fetch job result for the event job id.
            let known_result_addrs = RuntimeApi::storage()
                .jobs()
                .known_results(event.clone().role_type, event.clone().job_id);

            let maybe_result = client
                .storage()
                .at_latest()
                .await?
                .fetch(&known_result_addrs)
                .await?;

            let mut bridge_keys = Vec::new();
            // get evm bridges
            for (_, config) in self.webb_config.evm.iter() {
                let typed_chain_id =
                    webb_proposals::TypedChainId::Evm(config.chain_id);
                let bridge_key = BridgeKey::new(typed_chain_id);
                bridge_keys.push(bridge_key);
            }

            if let Some(phase_result) = maybe_result {
                if let JobResult::DKGPhaseTwo(result) = phase_result.result {
                    for bridge_key in &bridge_keys {
                        tracing::debug!(
                            %bridge_key,
                            ?result,
                            "Signaling Signature Bridge to transfer ownership",
                        );

                        tracing::event!(
                            target: webb_relayer_utils::probe::TARGET,
                            tracing::Level::DEBUG,
                            kind = %webb_relayer_utils::probe::Kind::SigningBackend,
                            backend = "SigningRules",
                            signal_bridge = %bridge_key,
                            public_key = %hex::encode(&result.data.0),
                            signature = %hex::encode(&result.signature.0),
                        );

                        // Todo enqueue transfer ownership calls
                    }
                }
            }
        }
        Ok(())
    }
}
