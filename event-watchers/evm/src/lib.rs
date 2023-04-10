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
use webb::evm::contract::protocol_solidity::{
    MaspContract, MaspContractEvents, MaspProxyContract,
    MaspProxyContractEvents, OpenVAnchorContract, OpenVAnchorContractEvents,
    VAnchorContract, VAnchorContractEvents,
};
use webb::evm::ethers::contract::Contract;
use webb::evm::ethers::prelude::Middleware;
use webb::evm::ethers::{providers, types};

pub mod signature_bridge_watcher;

#[doc(hidden)]
pub mod masp;
/// A module for listening on open vanchor events.
#[doc(hidden)]
pub mod open_vanchor;
/// A module for listening on vanchor events.
#[doc(hidden)]
pub mod vanchor;

#[cfg(test)]
mod tests;

use webb_event_watcher_traits::evm::{EventWatcher, WatchableContract};
use webb_relayer_store::SledStore;

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

#[derive(Clone, Debug)]
pub struct MaspContractWrapper<M>
where
    M: Middleware,
{
    pub config: webb_relayer_config::evm::MaspContractConfig,
    pub webb_config: webb_relayer_config::WebbRelayerConfig,
    pub contract: MaspContract<M>,
}

impl<M> MaspContractWrapper<M>
where
    M: Middleware,
{
    /// Creates a new MaspContractOverDKGWrapper.
    pub fn new(
        config: webb_relayer_config::evm::MaspContractConfig,
        webb_config: webb_relayer_config::WebbRelayerConfig,
        client: Arc<M>,
    ) -> Self {
        Self {
            contract: MaspContract::new(config.common.address, client),
            config,
            webb_config,
        }
    }
}

impl<M> ops::Deref for MaspContractWrapper<M>
where
    M: Middleware,
{
    type Target = Contract<M>;

    fn deref(&self) -> &Self::Target {
        &self.contract
    }
}

impl<M> WatchableContract for MaspContractWrapper<M>
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

#[derive(Clone, Debug)]
pub struct MaspProxyContractWrapper<M>
where
    M: Middleware,
{
    pub config: webb_relayer_config::evm::MaspProxyContractConfig,
    pub webb_config: webb_relayer_config::WebbRelayerConfig,
    pub contract: MaspProxyContract<M>,
}

impl<M> MaspProxyContractWrapper<M>
where
    M: Middleware,
{
    /// Creates a new MaspProxyContractOverDKGWrapper.
    pub fn new(
        config: webb_relayer_config::evm::MaspProxyContractConfig,
        webb_config: webb_relayer_config::WebbRelayerConfig,
        client: Arc<M>,
    ) -> Self {
        Self {
            contract: MaspProxyContract::new(config.common.address, client),
            config,
            webb_config,
        }
    }
}

impl<M> ops::Deref for MaspProxyContractWrapper<M>
where
    M: Middleware,
{
    type Target = Contract<M>;

    fn deref(&self) -> &Self::Target {
        &self.contract
    }
}

impl<M> WatchableContract for MaspProxyContractWrapper<M>
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

/// An Masp Contract Watcher that watches for the Anchor contract events and calls the event
/// handlers.
#[derive(Copy, Clone, Debug, Default)]
pub struct MaspContractWatcher;

#[derive(Copy, Clone, Debug, Default)]
pub struct MaspProxyContractWatcher;

#[async_trait::async_trait]
impl EventWatcher for VAnchorContractWatcher {
    const TAG: &'static str = "VAnchor Contract Watcher";

    type Contract = VAnchorContractWrapper<HttpProvider>;

    type Events = VAnchorContractEvents;

    type Store = SledStore;
}

#[async_trait::async_trait]
impl EventWatcher for MaspContractWatcher {
    const TAG: &'static str = "Masp Contract Watcher";

    type Contract = MaspContractWrapper<HttpProvider>;

    type Events = MaspContractEvents;

    type Store = SledStore;
}

#[async_trait::async_trait]
impl EventWatcher for MaspProxyContractWatcher {
    const TAG: &'static str = "Masp Contract Watcher";

    type Contract = MaspProxyContractWrapper<HttpProvider>;

    type Events = MaspProxyContractEvents;

    type Store = SledStore;
}

// OpenVAnchorContractWrapper contains VAnchorContract contract along with configurations for Anchor contract, and Relayer.
#[derive(Clone, Debug)]
pub struct OpenVAnchorContractWrapper<M>
where
    M: Middleware,
{
    pub config: webb_relayer_config::evm::VAnchorContractConfig,
    pub webb_config: webb_relayer_config::WebbRelayerConfig,
    pub contract: OpenVAnchorContract<M>,
}

impl<M> OpenVAnchorContractWrapper<M>
where
    M: Middleware,
{
    /// Creates a new OpenVAnchorContractOverDKGWrapper.
    pub fn new(
        config: webb_relayer_config::evm::VAnchorContractConfig,
        webb_config: webb_relayer_config::WebbRelayerConfig,
        client: Arc<M>,
    ) -> Self {
        Self {
            contract: OpenVAnchorContract::new(config.common.address, client),
            config,
            webb_config,
        }
    }
}

impl<M> ops::Deref for OpenVAnchorContractWrapper<M>
where
    M: Middleware,
{
    type Target = Contract<M>;

    fn deref(&self) -> &Self::Target {
        &self.contract
    }
}

impl<M> WatchableContract for OpenVAnchorContractWrapper<M>
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

/// An Open VAnchor Contract Watcher that watches for the Anchor contract events and calls the event
/// handlers.
#[derive(Copy, Clone, Debug, Default)]
pub struct OpenVAnchorContractWatcher;

#[async_trait::async_trait]
impl EventWatcher for OpenVAnchorContractWatcher {
    const TAG: &'static str = "Open VAnchor Contract Watcher";

    type Contract = OpenVAnchorContractWrapper<HttpProvider>;

    type Events = OpenVAnchorContractEvents;

    type Store = SledStore;
}
