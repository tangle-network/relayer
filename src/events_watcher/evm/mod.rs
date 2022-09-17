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
use super::*;
use crate::config;
use crate::store::sled::SledStore;
use std::ops;
use std::sync::Arc;
use std::time::Duration;
use webb::evm::contract::protocol_solidity::{
    VAnchorContract, VAnchorContractEvents,
};
use webb::evm::ethers::contract::Contract;
use webb::evm::ethers::prelude::Middleware;
use webb::evm::ethers::types;

pub mod signature_bridge_watcher;
pub mod vanchor_deposit_handler;
pub mod vanchor_leaves_handler;
pub mod vanchor_encrypted_outputs_handler;

#[doc(hidden)]
pub use signature_bridge_watcher::*;
#[doc(hidden)]
pub use vanchor_deposit_handler::*;
#[doc(hidden)]
pub use vanchor_leaves_handler::*;

// VAnchorContractWrapper contains VAnchorContract contract along with configurations for Anchor contract, and Relayer.
#[derive(Clone, Debug)]
pub struct VAnchorContractWrapper<M>
where
    M: Middleware,
{
    pub config: config::VAnchorContractConfig,
    pub webb_config: config::WebbRelayerConfig,
    pub contract: VAnchorContract<M>,
}

impl<M> VAnchorContractWrapper<M>
where
    M: Middleware,
{
    /// Creates a new VAnchorContractOverDKGWrapper.
    pub fn new(
        config: config::VAnchorContractConfig,
        webb_config: config::WebbRelayerConfig,
        client: Arc<M>,
    ) -> Self {
        Self {
            contract: VAnchorContract::new(config.common.address, client),
            config,
            webb_config,
        }
    }
}

impl<M> ops::Deref for VAnchorContractWrapper<M>
where
    M: Middleware,
{
    type Target = Contract<M>;

    fn deref(&self) -> &Self::Target {
        &self.contract
    }
}

impl<M> super::WatchableContract for VAnchorContractWrapper<M>
where
    M: Middleware,
{
    fn deployed_at(&self) -> types::U64 {
        self.config.common.deployed_at.into()
    }

    fn polling_interval(&self) -> Duration {
        Duration::from_millis(self.config.events_watcher.polling_interval)
    }

    fn max_blocks_per_step(&self) -> types::U64 {
        self.config.events_watcher.max_blocks_per_step.into()
    }

    fn print_progress_interval(&self) -> Duration {
        Duration::from_millis(
            self.config.events_watcher.print_progress_interval,
        )
    }
}

type HttpProvider = providers::Provider<providers::Http>;

/// An Anchor Contract Watcher that watches for the Anchor contract events and calls the event
/// handlers.
#[derive(Copy, Clone, Debug, Default)]
pub struct AnchorContractWatcher;

/// An VAnchor Contract Watcher that watches for the Anchor contract events and calls the event
/// handlers.
#[derive(Copy, Clone, Debug, Default)]
pub struct VAnchorContractWatcher;

#[async_trait::async_trait]
impl super::EventWatcher for VAnchorContractWatcher {
    const TAG: &'static str = "VAnchor Contract Watcher";

    type Contract = VAnchorContractWrapper<HttpProvider>;

    type Events = VAnchorContractEvents;

    type Store = SledStore;
}
