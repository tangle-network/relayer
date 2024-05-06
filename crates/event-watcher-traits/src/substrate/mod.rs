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

//! # Substrate Events Watcher Traits 🕸️

use futures::prelude::*;
use std::cmp;
use std::sync::Arc;
use std::time::Duration;
use tangle_subxt::subxt::{self, client::OnlineClientT, config::Header};
use webb_proposals::{
    ResourceId, SubstrateTargetSystem, TargetSystem, TypedChainId,
};
use webb_relayer_store::HistoryStore;

/// Event watching traits
mod event_watcher;
pub use event_watcher::*;

/// Type alias for Substrate block number.
pub type BlockNumberOf<T> = <<T as subxt::Config>::Hasher as Header>::Number;
