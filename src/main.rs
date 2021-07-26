#![deny(unsafe_code)]

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Context;
use directories_next::ProjectDirs;
use futures::Future;
use structopt::StructOpt;
use warp::Filter;

use crate::context::RelayerContext;

mod chains;
mod config;
mod context;
mod handler;

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
    let config = load_config(args.config_filename)?;
    let ctx = RelayerContext::new(config);
    let (addr, server) = build_relayer(ctx)?;
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

    let routes = ip_filter.or(info_filter); // will add more routes here.
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
