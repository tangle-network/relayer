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
//! # Relayer Events Watcher Module 🕸️
//!
//! A module that listens for events on a given chain.
//!
//! ## Overview
//!
//! Event watcher traits handle the syncing and listening of events for a given network.
//! The event watcher calls into a storage for handling of important state. The run implementation
//! of an event watcher polls for blocks. Implementations of the event watcher trait define an
//! action to take when the specified event is found in a block at the `handle_event` api.

#[cfg(feature = "evm")]
pub mod evm;
#[cfg(feature = "evm")]
pub use evm::{
    BridgeWatcher, EventHandler as EVMEventHandler,
    EventHandlerWithRetry as EVMEventHandlerWithRetry,
    EventWatcher as EVMEventWatcher,
};

#[cfg(feature = "substrate")]
pub mod substrate;
#[cfg(feature = "substrate")]
pub use substrate::{
    EventHandler as SubstrateEventHandler,
    EventHandlerWithRetry as SubstrateEventHandlerWithRetry,
    SubstrateEventWatcher,
};

#[cfg(test)]
mod tests;
