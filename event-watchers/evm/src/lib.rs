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

use std::ops;
use std::sync::Arc;
use std::time::Duration;
use webb::evm::contract::protocol_solidity::variable_anchor::{
    VAnchorContract, VAnchorContractEvents,
};

use webb::evm::ethers::contract::Contract;
use webb::evm::ethers::prelude::Middleware;
use webb::evm::ethers::types;

pub mod signature_bridge_watcher;

/// A module for listening on vanchor events.
#[doc(hidden)]
pub mod vanchor;

#[cfg(test)]
mod tests;

use webb_event_watcher_traits::evm::{EventWatcher, WatchableContract};
use webb_relayer_store::SledStore;
use webb_relayer_types::EthersTimeLagClient;

// VAnchorContractWrapper contains VAnchorContract contract along with configurations for Anchor contract, and Relayer.
#[derive(Clone, Debug)]
pub struct VAnchorContractWrapper<M>
where
    M: Middleware,
{
    pub config: webb_relayer_config::evm::VAnchorContractConfig,
    pub webb_config: webb_relayer_config::WebbRelayerConfig,
    pub contract: VAnchorContract<M>,
}

impl<M> VAnchorContractWrapper<M>
where
    M: Middleware,
{
    /// Creates a new VAnchorContractOverDKGWrapper.
    pub fn new(
        config: webb_relayer_config::evm::VAnchorContractConfig,
        webb_config: webb_relayer_config::WebbRelayerConfig,
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

impl<M> WatchableContract for VAnchorContractWrapper<M>
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

/// An VAnchor Contract Watcher that watches for the Anchor contract events and calls the event
/// handlers.
#[derive(Copy, Clone, Debug, Default)]
pub struct VAnchorContractWatcher;

#[async_trait::async_trait]
impl EventWatcher for VAnchorContractWatcher {
    const TAG: &'static str = "VAnchor Contract Watcher";

    type Contract = VAnchorContractWrapper<EthersTimeLagClient>;

    type Events = VAnchorContractEvents;

    type Store = SledStore;
}
