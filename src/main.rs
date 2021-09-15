#![deny(unsafe_code)]

use std::convert::TryFrom;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use directories_next::ProjectDirs;
use futures::Future;
use std::net::SocketAddr;
use structopt::StructOpt;
use warp::Filter;
use warp_real_ip::real_ip;
use webb::evm::ethers::providers;

use crate::context::RelayerContext;
use crate::events_watcher::*;

mod config;
mod context;
mod events_watcher;
mod handler;
mod store;

#[cfg(test)]
mod test_utils;

const PACKAGE_ID: [&str; 3] = ["tools", "webb", "webb-relayer"];
/// The Webb Relayer Command-line tool
///
/// Start the relayer from a config file:
///
///     $ webb-relayer -vvv -c <CONFIG_FILE_PATH>
#[derive(StructOpt)]
#[structopt(name = "Webb Relayer")]
struct Opts {
    /// A level of verbosity, and can be used multiple times
    #[structopt(short, long, parse(from_occurrences))]
    verbose: i32,
    /// File that contains configration.
    #[structopt(
        short = "c",
        long = "config-filename",
        value_name = "PATH",
        parse(from_os_str)
    )]
    config_filename: Option<PathBuf>,
}

#[paw::main]
#[tokio::main]
async fn main(args: Opts) -> anyhow::Result<()> {
    setup_logger(args.verbose)?;
    let config = load_config(args.config_filename.clone())?;
    let ctx = RelayerContext::new(config);
    let store = create_store(args.config_filename).await?;
    start_background_services(&ctx, Arc::new(store.clone())).await?;
    let (addr, server) = build_relayer(ctx.clone(), store)?;
    tracing::info!("Starting the server on {}", addr);
    // fire the server.
    let server_handle = tokio::spawn(server);

    match tokio::signal::ctrl_c().await {
        Ok(()) => {
            tracing::warn!("Shutting down...");
            // send shutdown signal to all of the application.
            ctx.shutdown();
            // also abort the server task
            server_handle.abort();
            tracing::info!("Shutting down...");
            tracing::info!("Clean Exit ..");
        }
        Err(err) => {
            tracing::error!("Unable to listen for shutdown signal: {}", err);
            // we also shut down in case of error
            ctx.shutdown();
            // Force shutdown.
            std::process::exit(1);
        }
    }
    Ok(())
}

fn setup_logger(verbosity: i32) -> anyhow::Result<()> {
    use tracing::Level;
    let log_level = match verbosity {
        0 => Level::ERROR,
        1 => Level::WARN,
        2 => Level::INFO,
        3 => Level::DEBUG,
        _ => Level::TRACE,
    };

    let env_filter = tracing_subscriber::EnvFilter::from_default_env()
        .add_directive(format!("webb_relayer={}", log_level).parse()?);
    tracing_subscriber::fmt()
        .with_target(true)
        .with_max_level(log_level)
        .with_env_filter(env_filter)
        .init();
    Ok(())
}

fn load_config<P>(
    config_filename: Option<P>,
) -> anyhow::Result<config::WebbRelayerConfig>
where
    P: AsRef<Path>,
{
    tracing::debug!("Getting default dirs for webb relayer");
    let dirs = ProjectDirs::from(
        crate::PACKAGE_ID[0],
        crate::PACKAGE_ID[1],
        crate::PACKAGE_ID[2],
    )
    .context("failed to get config")?;
    let config_path = match config_filename {
        Some(p) => p.as_ref().to_path_buf(),
        None => dirs.config_dir().join("config.toml"),
    };
    tracing::trace!("Loaded Config from {} ..", config_path.display());
    config::load(config_path).context("failed to load the config file")
}

fn build_relayer(
    ctx: RelayerContext,
    store: store::sled::SledLeafCache,
) -> anyhow::Result<(SocketAddr, impl Future<Output = ()> + 'static)> {
    let port = ctx.config.port;
    let ctx_arc = Arc::new(ctx.clone());
    let ctx_filter = warp::any().map(move || Arc::clone(&ctx_arc));
    // the websocket server.
    let ws_filter = warp::path("ws")
        .and(warp::ws())
        .and(ctx_filter.clone())
        .map(|ws: warp::ws::Ws, ctx: Arc<RelayerContext>| {
            ws.on_upgrade(|socket| async move {
                let _ = handler::accept_connection(ctx.as_ref(), socket).await;
            })
        });

    // get the ip of the caller.
    let proxy_addr = [127, 0, 0, 1].into();

    // First check the x-forwarded-for with 'real_ip' for reverse proxy setups
    let ip_filter = warp::path("ip")
        .and(warp::get())
        .and(real_ip(vec![proxy_addr]))
        .and_then(handler::handle_ip_info)
        .or(warp::path("ip")
            .and(warp::get())
            .and(warp::addr::remote())
            .and_then(handler::handle_socket_info));

    // relayer info
    let info_filter = warp::path("info")
        .and(warp::get())
        .and(ctx_filter)
        .and_then(handler::handle_relayer_info);

    let store = Arc::new(store);
    let store_filter = warp::any().map(move || Arc::clone(&store));
    let leaves_cache_filter = warp::path("leaves")
        .and(store_filter)
        .and(warp::path::param())
        .and_then(handler::handle_leaves_cache);

    let routes = ip_filter.or(info_filter).or(leaves_cache_filter); // will add more routes here.
    let http_filter = warp::path("api").and(warp::path("v1")).and(routes);

    let cors = warp::cors().allow_any_origin();
    let service = http_filter
        .or(ws_filter)
        .with(cors)
        .with(warp::trace::request());
    let mut shutdown_signal = ctx.shutdown_signal();
    let shutdown_signal = async move {
        shutdown_signal.recv().await;
    };
    warp::serve(service)
        .try_bind_with_graceful_shutdown(([0, 0, 0, 0], port), shutdown_signal)
        .map_err(Into::into)
}

async fn create_store<P>(
    path: Option<P>,
) -> anyhow::Result<store::sled::SledLeafCache>
where
    P: AsRef<Path>,
{
    let dirs = ProjectDirs::from(
        crate::PACKAGE_ID[0],
        crate::PACKAGE_ID[1],
        crate::PACKAGE_ID[2],
    )
    .context("failed to get config")?;
    let p = match path.as_ref() {
        Some(p) => p.as_ref().to_path_buf(),
        None => dirs.data_local_dir().to_path_buf(),
    };
    let db_path = match path.zip(p.parent()) {
        Some((_, parent)) => parent.join("store"),
        None => p.join("store"),
    };

    let store = store::sled::SledLeafCache::open(db_path)?;
    Ok(store)
}

async fn start_background_services(
    ctx: &RelayerContext,
    store: Arc<store::sled::SledLeafCache>,
) -> anyhow::Result<()> {
    // now we go through each chain, in our configuration
    for (chain_name, chain_config) in &ctx.config.evm {
        let provider = providers::Provider::<providers::Http>::try_from(
            chain_config.http_endpoint.as_str(),
        )?
        .interval(Duration::from_millis(6u64));
        let client = Arc::new(provider);
        // TODO(@shekohex): move every variant block into small a async functions.
        // Also, worth mentioning that we should move everything into another module.
        for contract in &chain_config.contracts {
            match contract {
                config::Contract::Anchor(config) => {
                    // check first if we should start the leaf watcher for this contract.
                    if !config.leaves_watcher.enabled {
                        tracing::warn!(
                            "Anchor Leaves watcher is disabled for {} on {}.",
                            config.common.address,
                            chain_name,
                        );
                        continue;
                    }
                    let wrapper = AnchorContractWrapper::new(
                        config.clone(),
                        client.clone(),
                    );
                    tracing::debug!(
                        "leaves watcher for {} ({}) Started.",
                        chain_name,
                        config.common.address,
                    );
                    let watcher = AnchorLeavesWatcher.run(
                        client.clone(),
                        store.clone(),
                        wrapper,
                    );
                    let mut shutdown_signal = ctx.shutdown_signal();
                    let task = async move {
                        // await the watcher to stop
                        // or we get a shutdown signal.
                        tokio::select! {
                            _ = watcher => {},
                            _ = shutdown_signal.recv() => {},
                        }
                    };
                    // kick off the watcher.
                    tokio::task::spawn(task);
                }
                config::Contract::Anchor2(config) => {
                    // check first if we should start the leaf watcher for this contract.
                    if !config.leaves_watcher.enabled {
                        tracing::warn!(
                            "Anchor Leaves watcher is disabled for {} on {}.",
                            config.common.address,
                            chain_name,
                        );
                        continue;
                    }
                    let wrapper = Anchor2ContractWrapper::new(
                        config.clone(),
                        client.clone(),
                    );
                    tracing::debug!(
                        "leaves watcher for {} ({}) Started.",
                        chain_name,
                        config.common.address,
                    );

                    const LEAVES_WATCHER: Anchor2LeavesWatcher =
                        Anchor2LeavesWatcher::new();
                    let leaves_watcher_task = LEAVES_WATCHER.run(
                        client.clone(),
                        store.clone(),
                        wrapper.clone(),
                    );

                    const BRIDGE_WATCHER: Anchor2BridgeWatcher =
                        Anchor2BridgeWatcher::new();
                    let bridge_watcher_task = BRIDGE_WATCHER.run(
                        client.clone(),
                        store.clone(),
                        wrapper.clone(),
                    );
                    let mut shutdown_signal = ctx.shutdown_signal();
                    let task = async move {
                        tokio::select! {
                            _ = leaves_watcher_task => {},
                            _ = bridge_watcher_task => {},
                            _ = shutdown_signal.recv() => {},
                        }
                    };
                    // kick off the watcher.
                    tokio::task::spawn(task);
                }
                config::Contract::Bridge(config) => {
                    let wrapper = BridgeContractWrapper::new(
                        config.clone(),
                        client.clone(),
                    );
                    tracing::debug!(
                        "bridge watcher for {} ({}) Started.",
                        chain_name,
                        config.common.address,
                    );

                    const BRIDGE_CONTRACT_WATCHER: BridgeContractWatcher =
                        BridgeContractWatcher;
                    let events_watcher_task = EventWatcher::run(
                        &BRIDGE_CONTRACT_WATCHER,
                        client.clone(),
                        store.clone(),
                        wrapper.clone(),
                    );
                    let cmd_handler_task = BridgeWatcher::run(
                        &BRIDGE_CONTRACT_WATCHER,
                        client.clone(),
                        store.clone(),
                        wrapper.clone(),
                    );
                    let mut shutdown_signal = ctx.shutdown_signal();
                    let task = async move {
                        tokio::select! {
                            _ = events_watcher_task => {},
                            _ = cmd_handler_task => {},
                            _ = shutdown_signal.recv() => {},
                        }
                    };
                    // kick off the watcher.
                    tokio::task::spawn(task);
                }
                config::Contract::GovernanceBravoDelegate(_) => {}
            }
        }
    }
    Ok(())
}
