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

/// A module for listening on JobResult Submissions event.
mod job_result_handler;
#[doc(hidden)]
pub use job_result_handler::*;
use tangle_subxt::subxt::events::StaticEvent;
use tangle_subxt::tangle_testnet_runtime::api::jobs::events::JobResultSubmitted;
use webb_event_watcher_traits::SubstrateEventWatcher;
use webb_relayer_utils::TangleRuntimeConfig;

/// The JobResultWatcher watches for the events from Jobs Pallet.
#[derive(Copy, Clone, Debug, Default)]
pub struct JobResultWatcher;

#[async_trait::async_trait]
impl SubstrateEventWatcher<TangleRuntimeConfig> for JobResultWatcher {
    const TAG: &'static str = "Job Pallet Event Watcher";

    const PALLET_NAME: &'static str = JobResultSubmitted::PALLET;

    type Store = webb_relayer_store::SledStore;
}
