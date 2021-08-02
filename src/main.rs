#![deny(unsafe_code)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use directories_next::ProjectDirs;
use futures::Future;
use structopt::StructOpt;
use warp::Filter;

use crate::chains::evm::EvmChain;
use crate::context::RelayerContext;

mod chains;
mod config;
mod context;
mod handler;
mod leaf_cache;

#[cfg(test)]
mod test_utils;

const PACKAGE_ID: [&str; 3] = ["tools", "webb", "webb-relayer"];
/// The Webb Relayer Command-line tool
///
/// Start the relayer from a config file:
///
///     $ webb-relayer -c <CONFIG_FILE_PATH>
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
    setup_logger(args.verbose);
    let config = load_config(args.config_filename.clone())?;
    let ctx = RelayerContext::new(config);
    let store = start_leave_cache_service(args.config_filename).await?;
    let (addr, server) = build_relayer(ctx, store)?;
    log::debug!("Starting the server on {}", addr);
    // fire the server.
    server.await;
    Ok(())
}

fn setup_logger(verbosity: i32) {
    let log_level = match verbosity {
        0 => log::LevelFilter::Error,
        1 => log::LevelFilter::Warn,
        2 => log::LevelFilter::Info,
        3 => log::LevelFilter::Debug,
        _ => log::LevelFilter::max(),
    };
    env_logger::builder()
        .format_timestamp(None)
        .filter_module("webb_relayer", log_level)
        .init();
}

fn load_config<P>(
    config_filename: Option<P>,
) -> anyhow::Result<config::WebbRelayerConfig>
where
    P: AsRef<Path>,
{
    log::debug!("Getting default dirs for webb relayer");
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
    log::trace!("Loaded Config from {} ..", config_path.display());
    config::load(config_path).context("failed to load the config file")
}

fn build_relayer(
    ctx: RelayerContext,
    store: leaf_cache::SledLeafCache,
) -> anyhow::Result<(SocketAddr, impl Future<Output = ()> + 'static)> {
    let port = ctx.config.port;
    let ctx = Arc::new(ctx);
    let ctx_filter = warp::any().map(move || Arc::clone(&ctx));
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
    let ip_filter = warp::path("ip")
        .and(warp::get())
        .and(warp::addr::remote())
        .and_then(handler::handle_ip_info);

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

    let ctrlc = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    let cors = warp::cors().allow_any_origin();
    let service = http_filter
        .or(ws_filter)
        .with(cors)
        .with(warp::trace::request());

    warp::serve(service)
        .try_bind_with_graceful_shutdown(([0, 0, 0, 0], port), ctrlc)
        .map_err(Into::into)
}

async fn start_leave_cache_service<P>(
    path: Option<P>,
) -> anyhow::Result<leaf_cache::SledLeafCache>
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
        Some((_, parent)) => parent.join("leaves"),
        None => p.join("leaves"),
    };

    let store = leaf_cache::SledLeafCache::open(db_path)?;
    // some macro magic to not repeat myself.
    macro_rules! start_network_watcher_for {
        ($network: ident) => {
            let contracts = chains::evm::$network::contracts()
                .into_keys()
                .collect::<Vec<_>>();
            for contract in contracts {
                let watcher = leaf_cache::LeavesWatcher::new(
                    chains::evm::$network::ws_endpoint(),
                    store.clone(),
                    contract,
                );
                let task = async move {
                    tokio::select! {
                        _ = watcher.watch() => {
                            log::warn!("watcher for {} stopped", stringify!($network));
                        },
                        _ = tokio::signal::ctrl_c() => {
                            log::debug!("Stopping the Task watcher for {}", stringify!($network));
                        }
                    };
                };
                tokio::task::spawn(task);
            }
        };
        ($($network: ident),+) => {
            $(
                start_network_watcher_for!($network);
            )+
        }
    }

    start_network_watcher_for!(Ganache, Beresheet, Harmony);
    Ok(store)
}
