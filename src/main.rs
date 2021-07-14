#![deny(unsafe_code)]

use std::path::PathBuf;

use anyhow::Context;
use directories_next::ProjectDirs;
use structopt::StructOpt;
use tokio::net::TcpListener;

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
    log::trace!("Loaded Config ..");
    let config =
        config::load(config_path).context("failed to load the config file")?;

    let addr = format!("0.0.0.0:{}", config.port);
    let ctx = context::RelayerContext::new(config);
    log::debug!("Starting the server on {}", addr);
    let socket = TcpListener::bind(addr).await?;

    // create a task for background sockets.
    let socket_task = async move {
        while let Ok((stream, _)) = socket.accept().await {
            log::debug!("Client Connected: {}", stream.peer_addr()?);
            tokio::spawn(handler::accept_connection(ctx.clone(), stream));
        }
        Result::<_, anyhow::Error>::Ok(())
    };

    let ctrl_c = tokio::signal::ctrl_c();
    // now we wait which of these would end first.
    tokio::select! {
        _ = socket_task => {
            log::warn!("Relayer Server Stopped.");
        },
        _ = ctrl_c => {
            log::info!("Stopping the Relayer Server.");
        }
    }
    Ok(())
}
