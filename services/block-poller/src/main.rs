//! A block poller using the `webb-relayer` framework.

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::sync::Arc;
use tokio::signal::unix;
use webb_relayer::context::RelayerContext;
use webb_relayer_config::cli::{create_store, load_config, setup_logger, Opts};

/// The main entry point for the relayer.
///
/// # Arguments
///
/// * `args` - The command line arguments.
#[paw::main]
#[tokio::main]
async fn main(args: Opts) -> anyhow::Result<()> {
    setup_logger(args.verbose)?;
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
        target: webb_relayer::probe::TARGET,
        tracing::Level::DEBUG,
        kind = %webb_relayer::probe::Kind::Lifecycle,
        started = true
    );
    // watch for signals
    let mut ctrlc_signal = unix::signal(unix::SignalKind::interrupt())?;
    let mut termination_signal = unix::signal(unix::SignalKind::terminate())?;
    let mut quit_signal = unix::signal(unix::SignalKind::quit())?;
    let shutdown = || {
        tracing::event!(
            target: webb_relayer::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %webb_relayer::probe::Kind::Lifecycle,
            shutdown = true
        );
        tracing::warn!("Shutting down...");
        // send shutdown signal to all of the application.
        ctx.shutdown();
        // also abort the server task
        server_handle.abort();
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
