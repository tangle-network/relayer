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

//! # Substrate Events Watcher Traits 🕸️

use futures::prelude::*;
use std::cmp;
use std::sync::Arc;
use std::time::Duration;
use webb::substrate::subxt::{self, client::OnlineClientT, config::Header};
use webb_proposals::{
    ResourceId, SubstrateTargetSystem, TargetSystem, TypedChainId,
};
use webb_relayer_store::HistoryStore;

/// Event watching traits
mod event_watcher;
pub use event_watcher::*;

/// Type alias for Substrate block number.
pub type BlockNumberOf<T> = <<T as subxt::Config>::Hasher as Header>::Number;
