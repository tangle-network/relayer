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

//! Webb Relayer Binary.
#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;
use std::time::Duration;
use tokio::signal::unix;
use tokio::time;
use webb_relayer::service::build_axum_services;

use webb_relayer_config::cli::{create_store, load_config, setup_logger, Opts};
use webb_relayer_context::RelayerContext;

/// The main entry point for the relayer.
///
/// # Arguments
///
/// * `args` - The command line arguments.
#[paw::main]
#[tokio::main]
async fn main(args: Opts) -> anyhow::Result<()> {
    setup_logger(args.verbose, "webb_relayer")?;
    match dotenv::dotenv() {
        Ok(_) => {
            tracing::trace!("Loaded .env file");
        }
        Err(e) => {
            tracing::warn!("Failed to load .env file: {}", e);
        }
    }

    // The configuration is validated and configured from the given directory
    let config = load_config(args.config_dir.clone())?;

    // The RelayerContext takes a configuration, and populates objects that are needed
    // throughout the lifetime of the relayer. Items such as wallets and providers, as well
    // as a convenient place to access the configuration.
    let ctx = RelayerContext::new(config);

    // persistent storage for the relayer
    let store = create_store(&args).await?;
    let cloned_store = store.clone();
    let metrics_clone = ctx.metrics.clone();

    // metric for data stored which is determined every 1 hour
    let sled_metric_task_handle = tokio::task::spawn(async move {
        let mut sled_data_metric_interval =
            time::interval(Duration::from_secs(3600));
        loop {
            sled_data_metric_interval.tick().await;
            // set data stored
            metrics_clone
                .lock()
                .await
                .total_amount_of_data_stored
                .set(cloned_store.get_data_stored_size() as f64);
        }
    });

    tokio::spawn(build_axum_services(ctx.clone()));

    // the build_web_relayer command sets up routing (endpoint queries / requests mapped to handled code)
    // so clients can interact with the relayer
    let (addr, server) =
        webb_relayer::service::build_web_services(ctx.clone(), store.clone())?;
    tracing::info!("Starting the server on {}", addr);
    // start the server.
    let server_handle = tokio::spawn(server);
    // start all background services.
    // this does not block, will fire the services on background tasks.
    webb_relayer::service::ignite(&ctx, Arc::new(store)).await?;
    tracing::event!(
        target: webb_relayer_utils::probe::TARGET,
        tracing::Level::DEBUG,
        kind = %webb_relayer_utils::probe::Kind::Lifecycle,
        started = true
    );
    // watch for signals
    let mut ctrlc_signal = unix::signal(unix::SignalKind::interrupt())?;
    let mut termination_signal = unix::signal(unix::SignalKind::terminate())?;
    let mut quit_signal = unix::signal(unix::SignalKind::quit())?;
    let shutdown = || {
        tracing::event!(
            target: webb_relayer_utils::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %webb_relayer_utils::probe::Kind::Lifecycle,
            shutdown = true
        );
        tracing::warn!("Shutting down...");
        // shut down storage fetching
        // send shutdown signal to all of the application.
        ctx.shutdown();
        // also abort the server task
        server_handle.abort();
        // abort get sled storage data task
        sled_metric_task_handle.abort();
        std::thread::sleep(std::time::Duration::from_millis(300));
        tracing::info!("Clean Exit ..");
    };
    tokio::select! {
        _ = ctrlc_signal.recv() => {
            tracing::warn!("Interrupted (Ctrl+C) ...");
            shutdown();
        },
        _ = termination_signal.recv() => {
            tracing::warn!("Got Terminate signal ...");
            shutdown();
        },
        _ = quit_signal.recv() => {
            tracing::warn!("Quitting ...");
            shutdown();
        },
    }
    Ok(())
}
