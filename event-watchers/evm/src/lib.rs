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
