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

/// A module for listening on proposal events.
mod proposal_signed_handler;
#[doc(hidden)]
pub use proposal_signed_handler::*;
/// A module for listening on DKG Governor Changes event.
mod public_key_changed_handler;
#[doc(hidden)]
pub use public_key_changed_handler::*;
use webb::substrate::subxt::events::StaticEvent;
use webb::substrate::subxt::PolkadotConfig;
use webb::substrate::tangle_runtime::api::{
    dkg::events::PublicKeySignatureChanged,
    dkg_proposal_handler::events::ProposalSigned,
};
use webb_event_watcher_traits::SubstrateEventWatcher;

/// The DKGMetadataWatcher watches for the events from Dkg Pallet.
#[derive(Copy, Clone, Debug, Default)]
pub struct DKGMetadataWatcher;

#[async_trait::async_trait]
impl SubstrateEventWatcher<PolkadotConfig> for DKGMetadataWatcher {
    const TAG: &'static str = "DKG Pallet Event Watcher";

    const PALLET_NAME: &'static str = PublicKeySignatureChanged::PALLET;

    type Store = webb_relayer_store::SledStore;
}

/// The DKGProposalHandlerWatcher watches for the events from Dkg Proposal Handler Pallet.
#[derive(Copy, Clone, Debug, Default)]
pub struct DKGProposalHandlerWatcher;

#[async_trait::async_trait]
impl SubstrateEventWatcher<PolkadotConfig> for DKGProposalHandlerWatcher {
    const TAG: &'static str = "DKG Proposal Handler Pallet Event Watcher";

    const PALLET_NAME: &'static str = ProposalSigned::PALLET;

    type Store = webb_relayer_store::SledStore;
}
