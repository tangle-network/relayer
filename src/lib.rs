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
//
#![deny(unsafe_code)]
#![warn(missing_docs)]

//! # Webb Relayer Crate 🕸️
//!
//! A crate used to relaying updates and transactions for the Webb Anchor protocol.
//!
//! ## Overview
//!
//! In the Webb Protocol, the relayer is a multi-faceted oracle, data relayer, and protocol
//! governance participant. Relayers fulfill the role of an oracle where the external data sources that
//! they listen to are the state of the anchors for a bridge. Relayers, as their name entails, relay
//! information for a connected set of Anchors on a bridge. This information is then used to update
//! the state of each Anchor and allow applications to reference, both privately and potentially not,
//! properties of data stored across the other connected Anchors.
//!
//! The relayer system is composed of three main components. Each of these components should be thought of as entirely
//! separate because they could be handled by different entities entirely.
//!
//!   1. Private transaction relaying (of user bridge transactions like Tornado Cash’s relayer)
//!   2. Data querying (for zero-knowledge proof generation)
//!   3. Data proposing and signature relaying (of DKG proposals)
//!
//! #### Private Transaction Relaying
//!
//! The relayer allows for submitting proofs for privacy-preserving transactions against the Mixer, Anchor and
//! VAnchor protocols. The users generate zero-knowledge proof data, format a proper payload, and submit
//! it to a compatible relayer for submission.
//!
//! #### Data Querying
//!
//! The relayer also supplements users who need to generate witness data for their zero-knowledge proofs.
//! The relayers cache the leaves of the trees of Mixer, Anchor or VAnchor that they are supporting.
//! This allows users to query for the leaf data faster than querying from a chain directly.
//!
//! #### Data Proposing and Signature Relaying
//!
//! The relayer is tasked with relaying signed data payloads from the DKG's activities and plays an important
//! role as it pertains to the Anchor Protocol. The relayer is responsible for submitting the unsigned and
//! signed anchor update proposals to and from the DKG before and after signing occurs.
//!
//! This role can be divided into two areas:
//! 1. Proposing
//! 2. Relaying
//!
//! The relayer is the main agent in the system who proposes anchor updates to the DKG for signing. That is,
//! the relayer acts as an oracle over the merkle trees of the Anchors and VAnchors. When new insertions into
//! the merkle trees occur, the relayer crafts an update proposal that is eventually proposed to the DKG for signing.
//!
//! The relayer is also responsible for relaying signed proposals. When anchor updates are signed, relayers are
//! tasked with submitting these signed payloads to the smart contract SignatureBridges that verify and handle
//! valid signed proposals. For all other signed proposals, the relayer is tasked with relaying these payloads
//! to the SignatureBridge instances and/or Governable instances.
//!
//! **The responsibility for a relayer to the DKG (governance system) can be summarized as follows:**
//!
//! The relayers act as proposers of proposals intended to be signed by the distributed key generation
//! protocol (DKG).
//!
//!  1. The relayers are listening to and proposing updates.
//!  2. The DKG is signing these updates using a threshold-signature scheme.
//!
//! We require a threshold of relayers (*really proposers*) to agree on the update in order to move the update
//! into a queue for the DKG to sign from.
//!
//! # Features
//!
//! There are several feature flags that control how much is available as part of the crate, both
//! `evm-runtime`, `substrate-runtime` are enabled by default.
//!
//! * `evm-runtime`: Enables the EVM runtime. By default, this is enabled.
//! * `substrate-runtime`: Enables the substrate runtime. By default, this is enabled.
//! * `integration-tests`: Enables integration tests. By default, this is disabled.

/// A module for managing the context of the relayer.
pub mod context;
/// A module that listens for events on a given chain.
pub mod events_watcher;
/// A module containing a collection of executable routines.
pub mod handler;
/// Metrics functionality
pub mod metric;
/// A module used for debugging relayer lifecycle, sync state, or other relayer state.
pub mod probe;
/// A module containing proposal signing backend (dkg and mocked).
pub mod proposal_signing_backend;
/// A module for starting long-running tasks for event watching.
pub mod service;
/// A module for managing the transaction queue for the relayer.
pub mod tx_queue;
/// Transaction relaying handlers
pub mod tx_relay;

pub use webb_relayer_utils::{Error, Result};
