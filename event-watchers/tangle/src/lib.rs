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

/// A module for listening on JobResult Submissions event.
mod job_result_handler;
#[doc(hidden)]
pub use job_result_handler::*;
use webb::substrate::subxt::events::StaticEvent;
use webb::substrate::tangle_runtime::api::jobs::events::JobResultSubmitted;
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
