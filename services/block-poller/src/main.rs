//! A block poller using the `webb-relayer` framework.

#![deny(unsafe_code)]
#![warn(missing_docs)]

use ethereum_types::U256;
use std::sync::Arc;
use tokio::signal::unix;
use webb_block_poller::start_block_poller_service;
use webb_relayer::service::Store;
use webb_relayer_config::cli::{create_store, load_config, setup_logger, Opts};
use webb_relayer_context::RelayerContext;
use webb_relayer_utils::Result;

/// Starts all background services for all chains configured in the config file.
///
/// Returns a future that resolves when all services are started successfully.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `store` -[Sled](https://sled.rs)-based database store
pub async fn ignite(
    ctx: &RelayerContext,
    store: Arc<Store>,
) -> crate::Result<()> {
    tracing::debug!(
        "Relayer configuration: {}",
        serde_json::to_string_pretty(&ctx.config)?
    );

    // now we go through each chain, in our configuration
    for chain_config in ctx.config.evm.values() {
        if !chain_config.enabled {
            continue;
        }
        let chain_name = &chain_config.name;
        let chain_id = U256::from(chain_config.chain_id);
        let provider = ctx.evm_provider(&chain_id.to_string()).await?;
        let client = Arc::new(provider);
        tracing::debug!(
            "Starting Background Services for ({}) chain. ({:?})",
            chain_name,
            chain_config.block_poller
        );

        if let Some(poller_config) = &chain_config.block_poller {
            tracing::debug!("Starting block relay ({:#?})", poller_config,);
            start_block_poller_service(
                ctx,
                chain_id,
                client,
                store.clone(),
                poller_config.clone(),
            )?;
        }
    }
    Ok(())
}

/// The main entry point for the relayer.
///
/// # Arguments
///
/// * `args` - The command line arguments.
#[paw::main]
#[tokio::main]
async fn main(args: Opts) -> anyhow::Result<()> {
    setup_logger(args.verbose, "webb_block_poller")?;
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
    tracing::trace!("Loaded config.. {:#?}", config);
    // The RelayerContext takes a configuration, and populates objects that are needed
    // throughout the lifetime of the relayer. Items such as wallets and providers, as well
    // as a convenient place to access the configuration.
    let ctx = RelayerContext::new(config);
    // persistent storage for the relayer
    let store = create_store(&args).await?;
    tracing::trace!("Created persistent storage..");
    // the build_web_relayer command sets up routing (endpoint queries / requests mapped to handled code)
    // so clients can interact with the relayer
    let (addr, server) =
        webb_relayer::service::build_web_services(ctx.clone(), store.clone())?;
    tracing::info!("Starting the server on {}", addr);
    // start the server.
    let server_handle = tokio::spawn(server);
    // start all background services.
    // this does not block, will fire the services on background tasks.
    ignite(&ctx, Arc::new(store)).await?;
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
