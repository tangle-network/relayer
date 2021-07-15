#![deny(unsafe_code)]

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use directories_next::ProjectDirs;
use structopt::StructOpt;
use warp::Filter;

use crate::context::RelayerContext;

mod chains;
mod config;
mod context;
mod handler;

#[cfg(all(test, feature = "integration-tests"))]
mod test;

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
    let log_level = match args.verbose {
        0 => log::LevelFilter::Error,
        1 => log::LevelFilter::Warn,
        2 => log::LevelFilter::Info,
        3 => log::LevelFilter::Debug,
        _ => log::LevelFilter::max(),
    };
    // setup logger
    env_logger::builder()
        .format_timestamp(None)
        .filter_module("webb_relayer", log_level)
        .init();
    log::debug!("Getting default dirs for webb relayer");
    let dirs = ProjectDirs::from(
        crate::PACKAGE_ID[0],
        crate::PACKAGE_ID[1],
        crate::PACKAGE_ID[2],
    )
    .context("failed to get config")?;
    let config_path = args
        .config_filename
        .unwrap_or_else(|| dirs.config_dir().join("config.toml"));
    log::trace!("Loaded Config from {} ..", config_path.display());
    let config =
        config::load(config_path).context("failed to load the config file")?;
    let port = config.port;
    let ctx = Arc::new(RelayerContext::new(config));
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

    let info_filter = warp::path("info")
        .and(warp::get())
        .and(ctx_filter)
        .and_then(handler::handle_relayer_info);

    let routes = ip_filter.or(info_filter); // will add more routes here.
    let http_filter = warp::path("api").and(warp::path("v1")).and(routes);

    let ctrlc = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    let service = http_filter.or(ws_filter).with(warp::trace::request());

    let (addr, server) = warp::serve(service)
        .try_bind_with_graceful_shutdown(([0, 0, 0, 0], port), ctrlc)?;

    log::debug!("Starting the server on {}", addr);
    // fire the server.
    server.await;
    Ok(())
}
