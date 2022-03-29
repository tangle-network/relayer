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
use std::convert::TryFrom;
use std::time::Duration;

use anyhow::Context;
use tokio::sync::broadcast;
use webb::evm::ethers::core::k256::SecretKey;
use webb::evm::ethers::prelude::*;
use webb::substrate::subxt;
use webb::substrate::subxt::sp_core::sr25519::Pair as Sr25519Pair;

use crate::config;
/// RelayerContext contains Relayer's configuration and shutdown signal.
#[derive(Clone)]
pub struct RelayerContext {
    pub config: config::WebbRelayerConfig,
    /// Broadcasts a shutdown signal to all active connections.
    ///
    /// The initial `shutdown` trigger is provided by the `run` caller. The
    /// server is responsible for gracefully shutting down active connections.
    /// When a connection task is spawned, it is passed a broadcast receiver
    /// handle. When a graceful shutdown is initiated, a `()` value is sent via
    /// the broadcast::Sender. Each active connection receives it, reaches a
    /// safe terminal state, and completes the task.
    notify_shutdown: broadcast::Sender<()>,
}

impl RelayerContext {
    /// Creates a new RelayerContext.
    pub fn new(config: config::WebbRelayerConfig) -> Self {
        let (notify_shutdown, _) = broadcast::channel(2);
        Self {
            config,
            notify_shutdown,
        }
    }
    /// Returns a broadcast receiver handle for the shutdown signal.
    pub fn shutdown_signal(&self) -> Shutdown {
        Shutdown::new(self.notify_shutdown.subscribe())
    }
    /// Sends a shutdown signal to all subscribed tasks/connections.
    pub fn shutdown(&self) {
        let _ = self.notify_shutdown.send(());
    }
    /// evm_provider returns an Ethers provider for the configured EVM chain.
    pub async fn evm_provider(
        &self,
        chain_name: &str,
    ) -> anyhow::Result<Provider<Http>> {
        let chain_config = self.config.evm.get(chain_name).context(format!(
            "Chain {} not configured or enabled",
            chain_name
        ))?;
        let provider = Provider::try_from(chain_config.http_endpoint.as_str())?
            .interval(Duration::from_millis(5u64));
        Ok(provider)
    }
    /// evm_wallet returns an Ethers wallet for the configured EVM chain.
    pub async fn evm_wallet(
        &self,
        chain_name: &str,
    ) -> anyhow::Result<LocalWallet> {
        let chain_config = self.config.evm.get(chain_name).context(format!(
            "Chain {} not configured or enabled",
            chain_name
        ))?;
        let key = SecretKey::from_bytes(chain_config.private_key.as_bytes())?;
        let chain_id = chain_config.chain_id;
        let wallet = LocalWallet::from(key).with_chain_id(chain_id);
        Ok(wallet)
    }
    /// substrate_provider returns a Substrate provider for the configured
    pub async fn substrate_provider<C: subxt::Config>(
        &self,
        node_name: &str,
    ) -> anyhow::Result<subxt::Client<C>> {
        let node_config = self
            .config
            .substrate
            .get(node_name)
            .context(format!("Node {} not configured or enabled", node_name))?;
        let client = subxt::ClientBuilder::new()
            .set_url(node_config.ws_endpoint.as_str())
            .build()
            .await
            .context(format!(
                "connecting to {} using {}",
                node_name, node_config.ws_endpoint
            ))?;
        Ok(client)
    }
    /// substrate_wallet returns a Substrate wallet for the configured Substrate chain.
    pub async fn substrate_wallet(
        &self,
        node_name: &str,
    ) -> anyhow::Result<Sr25519Pair> {
        let node_config = self
            .config
            .substrate
            .get(node_name)
            .cloned()
            .context(format!("Node {} not configured or enabled", node_name))?;
        Ok(node_config.suri.into())
    }
}

/// Listens for the server shutdown signal.
///
/// Shutdown is signalled using a `broadcast::Receiver`. Only a single value is
/// ever sent. Once a value has been sent via the broadcast channel, the server
/// should shutdown.
///
/// The `Shutdown` struct listens for the signal and tracks that the signal has
/// been received. Callers may query for whether the shutdown signal has been
/// received or not.
#[derive(Debug)]
pub struct Shutdown {
    /// `true` if the shutdown signal has been received
    shutdown: bool,

    /// The receive half of the channel used to listen for shutdown.
    notify: broadcast::Receiver<()>,
}

impl Shutdown {
    /// Create a new `Shutdown` backed by the given `broadcast::Receiver`.
    pub fn new(notify: broadcast::Receiver<()>) -> Shutdown {
        Shutdown {
            shutdown: false,
            notify,
        }
    }

    /// Receive the shutdown notice, waiting if necessary.
    pub async fn recv(&mut self) {
        // If the shutdown signal has already been received, then return
        // immediately.
        if self.shutdown {
            return;
        }

        // Cannot receive a "lag error" as only one value is ever sent.
        let _ = self.notify.recv().await;

        // Remember that the signal has been received.
        self.shutdown = true;
    }
}
