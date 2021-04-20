#![deny(unsafe_code)]

use std::path::PathBuf;

use anyhow::Context;
use directories_next::ProjectDirs;
use runtime::subxt::sp_core::crypto::Pair;
use runtime::subxt::sp_core::sr25519;
use runtime::subxt::{Client, ClientBuilder, PairSigner};
use runtime::WebbRuntime;
use structopt::StructOpt;
use tokio::net::TcpListener;

mod config;
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
    /// Set the Node Url where we will connect to.
    #[structopt(
        long = "node-url",
        default_value = "ws://127.0.0.1:9944",
        env = "WEBB_NODE_URL",
        parse(try_from_str = url::Url::parse)
    )]
    url: url::Url,
    /// File that contains configration.
    #[structopt(
        short = "c",
        long = "config-filename",
        value_name = "PATH",
        parse(from_os_str)
    )]
    config_filename: Option<PathBuf>,
}

#[derive(Clone)]
pub struct RelayerContext {
    pair: PairSigner<WebbRuntime, sr25519::Pair>,
    client: Client<WebbRuntime>,
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
    let signer = sr25519::Pair::from_string(&config.suri, None)
        .ok()
        .context("failed to load the singer from suri")?;
    log::info!("using {} as an account", signer.public());
    let pair = PairSigner::new(signer);
    log::debug!("building the RPC client and connecting to the node...");
    log::debug!("connecting to: {}", args.url);
    let client = ClientBuilder::new()
        .set_url(args.url.as_str())
        .build()
        .await
        .context("failed to connect to the node")?;

    log::debug!("Connected!");
    let ctx = RelayerContext { pair, client };
    let addr = format!("0.0.0.0:{}", config.port);
    log::debug!("Starting the server on {}", addr);
    let socket = TcpListener::bind(addr).await?;
    while let Ok((stream, _)) = socket.accept().await {
        log::debug!("Client Connected: {}", stream.peer_addr()?);
        tokio::spawn(handler::accept_connection(ctx.clone(), stream));
    }
    Ok(())
}
