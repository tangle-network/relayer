use std::convert::TryFrom;
use std::time::Duration;

use anyhow::Context;
use tokio::sync::broadcast;
use webb::evm::ethers::core::k256::SecretKey;
use webb::evm::ethers::prelude::*;

use crate::config;

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
    pub fn new(config: config::WebbRelayerConfig) -> Self {
        let (notify_shutdown, _) = broadcast::channel(2);
        Self {
            config,
            notify_shutdown,
        }
    }

    pub fn shutdown_signal(&self) -> Shutdown {
        Shutdown::new(self.notify_shutdown.subscribe())
    }

    pub fn shutdown(&self) {
        let _ = self.notify_shutdown.send(());
    }

    pub async fn evm_provider(
        &self,
        chain_name: &str,
    ) -> anyhow::Result<Provider<Http>> {
        let chain_config = self
            .config
            .evm
            .get(chain_name)
            .context("this chain not configured")?;
        let provider = Provider::try_from(chain_config.http_endpoint.as_str())?
            .interval(Duration::from_millis(5u64));
        Ok(provider)
    }

    pub async fn evm_wallet(
        &self,
        chain_name: &str,
    ) -> anyhow::Result<LocalWallet> {
        let chain_config = self
            .config
            .evm
            .get(chain_name)
            .context("this chain not configured")?;
        let key = SecretKey::from_bytes(chain_config.private_key.as_bytes())?;
        let chain_id = chain_config.chain_id;
        let wallet = LocalWallet::from(key).with_chain_id(chain_id);
        Ok(wallet)
    }

    pub fn fee_percentage(&self, chain_name: &str) -> anyhow::Result<f64> {
        let chain_config = self
            .config
            .evm
            .get(chain_name)
            .context("this chain not configured")?;
        Ok(chain_config.withdraw_fee_percentage)
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
