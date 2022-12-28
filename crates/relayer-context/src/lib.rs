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
//! # Relayer Context Module 🕸️
//!
//! A module for managing the context of the relayer.
use std::convert::TryFrom;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{broadcast, Mutex};

#[cfg(feature = "evm")]
use webb::evm::ethers::core::k256::SecretKey;
#[cfg(feature = "evm")]
use webb::evm::ethers::prelude::*;

#[cfg(feature = "substrate")]
use webb::substrate::subxt;
#[cfg(feature = "substrate")]
use webb::substrate::subxt::ext::sp_core::sr25519::Pair as Sr25519Pair;

use webb_relayer_utils::metric::{self, Metrics};

/// RelayerContext contains Relayer's configuration and shutdown signal.
#[derive(Clone)]
pub struct RelayerContext {
    /// The configuration of the relayer.
    pub config: webb_relayer_config::WebbRelayerConfig,
    /// Broadcasts a shutdown signal to all active connections.
    ///
    /// The initial `shutdown` trigger is provided by the `run` caller. The
    /// server is responsible for gracefully shutting down active connections.
    /// When a connection task is spawned, it is passed a broadcast receiver
    /// handle. When a graceful shutdown is initiated, a `()` value is sent via
    /// the broadcast::Sender. Each active connection receives it, reaches a
    /// safe terminal state, and completes the task.
    notify_shutdown: broadcast::Sender<()>,
    /// Represents the metrics for the relayer
    pub metrics: Arc<Mutex<metric::Metrics>>,
}

impl RelayerContext {
    /// Creates a new RelayerContext.
    pub fn new(config: webb_relayer_config::WebbRelayerConfig) -> Self {
        let (notify_shutdown, _) = broadcast::channel(2);
        let metrics = Arc::new(Mutex::new(Metrics::new()));
        Self {
            config,
            notify_shutdown,
            metrics,
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
    /// Returns a new `EthereumProvider` for the relayer.
    ///
    /// # Arguments
    ///
    /// * `chain_id` - A string representing the chain id.
    #[cfg(feature = "evm")]
    pub async fn evm_provider(
        &self,
        chain_id: &str,
    ) -> webb_relayer_utils::Result<Provider<Http>> {
        let chain_config = self.config.evm.get(chain_id).ok_or_else(|| {
            webb_relayer_utils::Error::ChainNotFound {
                chain_id: chain_id.to_string(),
            }
        })?;
        let provider = Provider::try_from(chain_config.http_endpoint.as_str())?
            .interval(Duration::from_millis(5u64));
        Ok(provider)
    }
    /// Sets up and returns an EVM wallet for the relayer.
    ///
    /// # Arguments
    ///
    /// * `chain_id` - A string representing the chain id.
    #[cfg(feature = "evm")]
    pub async fn evm_wallet(
        &self,
        chain_name: &str,
    ) -> webb_relayer_utils::Result<LocalWallet> {
        let chain_config =
            self.config.evm.get(chain_name).ok_or_else(|| {
                webb_relayer_utils::Error::ChainNotFound {
                    chain_id: chain_name.to_string(),
                }
            })?;
        let private_key = chain_config
            .private_key
            .as_ref()
            .ok_or(webb_relayer_utils::Error::MissingSecrets)?;
        let key = SecretKey::from_be_bytes(private_key.as_bytes())?;
        let chain_id = chain_config.chain_id;
        let wallet = LocalWallet::from(key).with_chain_id(chain_id);
        Ok(wallet)
    }
    /// Sets up and returns a Substrate client for the relayer.
    ///
    /// # Arguments
    ///
    /// * `chain_id` - A string representing the chain ID.
    #[cfg(feature = "substrate")]
    pub async fn substrate_provider<C: subxt::Config>(
        &self,
        chain_id: &str,
    ) -> webb_relayer_utils::Result<subxt::OnlineClient<C>> {
        let node_config =
            self.config.substrate.get(chain_id).ok_or_else(|| {
                webb_relayer_utils::Error::NodeNotFound {
                    chain_id: chain_id.to_string(),
                }
            })?;
        let client = subxt::OnlineClient::<C>::from_url(
            node_config.ws_endpoint.to_string(),
        )
        .await?;
        Ok(client)
    }
    /// Sets up and returns a Substrate wallet for the relayer.
    ///
    /// # Arguments
    ///
    /// * `chain_id` - A string representing the chain ID.
    #[cfg(feature = "substrate")]
    pub async fn substrate_wallet(
        &self,
        chain_id: &str,
    ) -> webb_relayer_utils::Result<Sr25519Pair> {
        let node_config = self
            .config
            .substrate
            .get(chain_id)
            .cloned()
            .ok_or_else(|| webb_relayer_utils::Error::NodeNotFound {
                chain_id: chain_id.to_string(),
            })?;
        let suri_key = node_config
            .suri
            .ok_or(webb_relayer_utils::Error::MissingSecrets)?;
        Ok(suri_key.into())
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
