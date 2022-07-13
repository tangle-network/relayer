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
use super::*;
use std::sync::Arc;
use ethereum_types::U256;
use crate::context::RelayerContext;
use crate::events_watcher::evm::*;
use super::util as ServiceUtil;
use crate::events_watcher::EventWatcher;
/// Starts the event watcher for EVM Anchor events.
///
/// Returns Ok(()) if successful, or an error if not.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `config` - Anchor contract configuration
/// * `client` - EVM Chain api client
/// * `store` -[Sled](https://sled.rs)-based database store
async fn start_evm_anchor_events_watcher(
    ctx: &RelayerContext,
    config: &AnchorContractConfig,
    chain_id: U256,
    client: Arc<Client>,
    store: Arc<Store>,
) -> anyhow::Result<()> {
    if !config.events_watcher.enabled {
        tracing::warn!(
            "Anchor events watcher is disabled for ({}).",
            config.common.address,
        );
        return Ok(());
    }
    let wrapper = AnchorContractWrapper::new(
        config.clone(),
        ctx.config.clone(), // the original config to access all networks.
        client.clone(),
    );
    let mut shutdown_signal = ctx.shutdown_signal();
    let contract_address = config.common.address;
    let my_ctx = ctx.clone();
    let my_config = config.clone();
    let task = async move {
        tracing::debug!(
            "Anchor events watcher for ({}) Started.",
            contract_address,
        );
        let proposal_signing_backend = ServiceUtil::make_proposal_signing_backend(
            &my_ctx,
            store.clone(),
            chain_id,
            my_config.linked_anchors,
            my_config.proposal_signing_backend,
        )
        .await?;
        match proposal_signing_backend {
            ProposalSigningBackendSelector::Dkg(backend) => {
                let watcher = AnchorWatcher::new(backend);
                let anchor_watcher_task = watcher.run(client, store, wrapper);
                tokio::select! {
                    _ = anchor_watcher_task => {
                        tracing::warn!(
                            "Anchor watcher task stopped for ({})",
                            contract_address,
                        );
                    },
                    _ = shutdown_signal.recv() => {
                        tracing::trace!(
                            "Stopping Anchor watcher for ({})",
                            contract_address,
                        );
                    },
                }
            }
            ProposalSigningBackendSelector::Mocked(backend) => {
                let watcher = AnchorWatcher::new(backend);
                let anchor_watcher_task = watcher.run(client, store, wrapper);
                tokio::select! {
                    _ = anchor_watcher_task => {
                        tracing::warn!(
                            "Anchor watcher task stopped for ({})",
                            contract_address,
                        );
                    },
                    _ = shutdown_signal.recv() => {
                        tracing::trace!(
                            "Stopping Anchor watcher for ({})",
                            contract_address,
                        );
                    },
                }
            }
            ProposalSigningBackendSelector::None => {
                tracing::warn!(
                    "Proposal Signing Backend not configured for ({})",
                    contract_address,
                );
            }
        };

        Result::<_, anyhow::Error>::Ok(())
    };
    // kick off the watcher.
    tokio::task::spawn(task);

    Ok(())
}
