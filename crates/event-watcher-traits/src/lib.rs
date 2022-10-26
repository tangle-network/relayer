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
pub use evm::*;

#[cfg(feature = "substrate")]
pub mod substrate;
#[cfg(feature = "substrate")]
pub use substrate::*;

#[cfg(test)]
mod tests;
