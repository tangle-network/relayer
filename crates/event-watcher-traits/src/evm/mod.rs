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

#![warn(missing_docs)]
//! # EVM Events Watcher Traits 🕸️

use futures::prelude::*;
use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use webb::evm::ethers::{
    contract, providers::Middleware, types, types::transaction,
};
use webb_proposals::{ResourceId, TargetSystem, TypedChainId};
use webb_relayer_context::RelayerContext;
use webb_relayer_store::queue::QueueStore;
use webb_relayer_store::sled::SledQueueKey;
use webb_relayer_store::{
    BridgeCommand, BridgeKey, EventHashStore, HistoryStore,
};
use webb_relayer_utils::metric;

/// Event watching traits
mod event_watcher;
pub use event_watcher::*;

/// Bridge watching traits
mod bridge_watcher;
pub use bridge_watcher::*;
