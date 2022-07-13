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
use crate::config::*;
use webb::substrate::subxt;
use webb::evm::ethers::providers;
use webb::substrate::dkg_runtime::api::RuntimeApi as DkgRuntimeApi;
use crate::store::sled::SledStore;
use crate::proposal_signing_backend::*;
/// Anchor watcher service.
mod anchor_watcher_service;
#[doc(hidden)]
pub use anchor_watcher_service::*;
/// utility file
mod util;


/// Type alias for providers
type Client = providers::Provider<providers::Http>;

/// Type alias for the DKG RuntimeApi
type DkgRuntime = DkgRuntimeApi<
    subxt::DefaultConfig,
    subxt::PolkadotExtrinsicParams<subxt::DefaultConfig>,
>;
/// Type alias for [Sled](https://sled.rs)-based database store
type Store = crate::store::sled::SledStore;

// utility for selecting proposal signing backend
#[allow(clippy::large_enum_variant)]
pub enum ProposalSigningBackendSelector {
    None,
    Mocked(MockedProposalSigningBackend<SledStore>),
    Dkg(DkgProposalSigningBackend<DkgRuntime, subxt::DefaultConfig>),
}