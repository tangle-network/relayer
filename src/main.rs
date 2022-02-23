#![deny(unsafe_code)]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use directories_next::ProjectDirs;
use futures::Future;
use std::net::SocketAddr;
use structopt::StructOpt;
use tokio::signal::unix;
use warp::Filter;
use warp_real_ip::real_ip;

use crate::context::RelayerContext;

mod config;
mod context;
mod events_watcher;
mod handler;
mod probe;
mod service;
mod store;
mod tx_queue;
mod utils;

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
    /// Directory that contains configration files.
    #[structopt(
        short = "c",
        long = "config-dir",
        value_name = "PATH",
        parse(from_os_str)
    )]
    config_dir: Option<PathBuf>,
    /// Create the Database Store in a temporary directory.
    /// and will be deleted when the process exits.
    #[structopt(long)]
    tmp: bool,
}

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
    let config = load_config(args.config_dir.clone())?;
    let ctx = RelayerContext::new(config);
    let store = create_store(&args).await?;
    let (addr, server) = build_relayer(ctx.clone(), store.clone())?;
    tracing::info!("Starting the server on {}", addr);
    // fire the server.
    let server_handle = tokio::spawn(server);
    // start all background services.
    // this does not block, will fire the services on background tasks.
    service::ignite(&ctx, Arc::new(store)).await?;
    tracing::event!(
        target: crate::probe::TARGET,
        tracing::Level::DEBUG,
        kind = %crate::probe::Kind::Lifecycle,
        started = true
    );
    // watch for signals
    let mut ctrlc_signal = unix::signal(unix::SignalKind::interrupt())?;
    let mut termination_signal = unix::signal(unix::SignalKind::terminate())?;
    let mut quit_signal = unix::signal(unix::SignalKind::quit())?;
    let shutdown = || {
        tracing::event!(
            target: crate::probe::TARGET,
            tracing::Level::DEBUG,
            kind = %crate::probe::Kind::Lifecycle,
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
    let logger = tracing_subscriber::fmt()
        .with_target(true)
        .with_max_level(log_level)
        .with_env_filter(env_filter);
    // if we are not compiling for integration tests, we should use pretty logs
    #[cfg(not(feature = "integration-tests"))]
    let logger = logger.pretty();
    // otherwise, we should use json, which is easy to parse.
    #[cfg(feature = "integration-tests")]
    let logger = logger.json();

    logger.init();
    Ok(())
}

fn load_config<P>(
    config_dir: Option<P>,
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
    let path = match config_dir {
        Some(p) => p.as_ref().to_path_buf(),
        None => dirs.config_dir().to_path_buf(),
    };
    // return an error if the path is not a directory.
    if !path.is_dir() {
        return Err(anyhow::anyhow!("{} is not a directory", path.display()));
    }
    tracing::trace!("Loading Config from {} ..", path.display());
    config::load(path)
}

fn build_relayer(
    ctx: RelayerContext,
    store: store::sled::SledStore,
) -> anyhow::Result<(SocketAddr, impl Future<Output = ()> + 'static)> {
    let port = ctx.config.port;
    let ctx_arc = Arc::new(ctx.clone());
    let ctx_filter = warp::any().map(move || Arc::clone(&ctx_arc)).boxed();
    // the websocket server.
    let ws_filter = warp::path("ws")
        .and(warp::ws())
        .and(ctx_filter.clone())
        .map(|ws: warp::ws::Ws, ctx: Arc<RelayerContext>| {
            ws.on_upgrade(|socket| async move {
                let _ = handler::accept_connection(ctx.as_ref(), socket).await;
            })
        })
        .boxed();

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
            .and_then(handler::handle_socket_info))
        .boxed();

    // relayer info
    let info_filter = warp::path("info")
        .and(warp::get())
        .and(ctx_filter)
        .and_then(handler::handle_relayer_info)
        .boxed();

    let store = Arc::new(store);
    let store_filter = warp::any().map(move || Arc::clone(&store)).boxed();
    let leaves_cache_filter = warp::path("leaves")
        .and(store_filter)
        .and(warp::path::param())
        .and(warp::path::param())
        .and_then(handler::handle_leaves_cache)
        .boxed();

    let routes = ip_filter.or(info_filter).or(leaves_cache_filter).boxed(); // will add more routes here.
    let http_filter =
        warp::path("api").and(warp::path("v1")).and(routes).boxed();

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

async fn create_store(opts: &Opts) -> anyhow::Result<store::sled::SledStore> {
    // check if we shall use the temp dir.
    if opts.tmp {
        tracing::debug!("Using temp dir for store");
        let store = store::sled::SledStore::temporary()?;
        return Ok(store);
    }
    let dirs = ProjectDirs::from(
        crate::PACKAGE_ID[0],
        crate::PACKAGE_ID[1],
        crate::PACKAGE_ID[2],
    )
    .context("failed to get config")?;
    let p = match opts.config_dir.as_ref() {
        Some(p) => p.to_path_buf(),
        None => dirs.data_local_dir().to_path_buf(),
    };
    let db_path = match opts.config_dir.as_ref().zip(p.parent()) {
        Some((_, parent)) => parent.join("store"),
        None => p.join("store"),
    };

    let store = store::sled::SledStore::open(db_path)?;
    Ok(store)
}
