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

mod signature_bridge_watcher;
mod vanchor_deposit_handler;
mod vanchor_encrypted_output_handler;
mod vanchor_leaves_handler;

#[doc(hidden)]
pub use signature_bridge_watcher::*;
#[doc(hidden)]
pub use vanchor_deposit_handler::*;
#[doc(hidden)]
pub use vanchor_encrypted_output_handler::*;
#[doc(hidden)]
pub use vanchor_leaves_handler::*;
use webb::substrate::subxt::events::StaticEvent;
use webb::substrate::{
    protocol_substrate_runtime::api::v_anchor_bn254::events::Transaction,
    subxt::{OnlineClient, SubstrateConfig},
};
use webb_event_watcher_traits::SubstrateEventWatcher;
use webb_relayer_store::SledStore;

#[derive(Copy, Clone, Debug, Default)]
pub struct SubstrateVAnchorEventWatcher;

#[async_trait::async_trait]
impl SubstrateEventWatcher<SubstrateConfig> for SubstrateVAnchorEventWatcher {
    const TAG: &'static str = "Substrate VAnchor Event Watcher";

    const PALLET_NAME: &'static str = Transaction::PALLET;

    type Client = OnlineClient<SubstrateConfig>;

    type Store = SledStore;
}
