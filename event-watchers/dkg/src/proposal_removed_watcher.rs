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
use webb::substrate::dkg_runtime;
use webb::substrate::dkg_runtime::api::dkg_proposal_handler;
use webb::substrate::dkg_runtime::api::runtime_types::webb_proposals::header::TypedChainId;
use webb::substrate::subxt::{self, OnlineClient};
use webb_event_watcher_traits::SubstrateEventWatcher;
use webb_relayer_store::sled::{SledQueueKey, SledStore};
use webb_relayer_store::{BridgeCommand, BridgeKey, QueueStore};
use webb_relayer_utils::metric;

use webb_event_watcher_traits::substrate::BlockNumberOf;

/// A ProposalHandler watcher for the DKG Substrate runtime.
/// It watches for the `ProposalSigned` event and sends the proposal to the signature bridge.
#[derive(Copy, Clone, Debug, Default)]
pub struct ProposalRemovedWatcher;

#[async_trait::async_trait]
impl SubstrateEventWatcher for ProposalRemovedWatcher {
    const TAG: &'static str = "DKG Proposal Removed Watcher";

    const PALLET_NAME: &'static str = "DKGProposalHandler";

    type RuntimeConfig = subxt::PolkadotConfig;

    type Client = OnlineClient<Self::RuntimeConfig>;

    type Event = dkg_runtime::api::Event;

    type FilteredEvent = dkg_proposal_handler::events::ProposalRemoved;

    type Store = SledStore;

    async fn handle_event(
        &self,
        store: Arc<Self::Store>,
        _api: Arc<Self::Client>,
        (event, block_number): (Self::FilteredEvent, BlockNumberOf<Self>),
        metrics: Arc<Mutex<metric::Metrics>>,
    ) -> webb_relayer_utils::Result<()> {
        tracing::event!(
            target: webb_relayer_utils::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %webb_relayer_utils::probe::Kind::SigningBackend,
            backend = "DKG",
            ty = "  ",
            ?event.target_chain,
            ?event.key,
            ?block_number,
        );
        
        
        Ok(())
    }
}
