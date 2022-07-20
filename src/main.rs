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
//
#![deny(unsafe_code)]
#![warn(missing_docs)]

//! # Webb Relayer Crate 🕸️
//!
//! A crate used to relaying updates and transactions for the Webb Anchor protocol.
//!
//! ## Overview
//!
//! In the Webb Protocol, the relayer is a multi-faceted oracle, data relayer, and protocol
//! governance participant. Relayers fulfill the role of an oracle where the external data sources that
//! they listen to are the state of the anchors for a bridge. Relayers, as their name entails, relay
//! information for a connected set of Anchors on a bridge. This information is then used to update
//! the state of each Anchor and allow applications to reference, both privately and potentially not,
//! properties of data stored across the other connected Anchors.
//!
//! The relayer system is composed of three main components. Each of these components should be thought of as entirely
//! separate because they could be handled by different entities entirely.
//!
//!   1. Private transaction relaying (of user bridge transactions like Tornado Cash’s relayer)
//!   2. Data querying (for zero-knowledge proof generation)
//!   3. Data proposing and signature relaying (of DKG proposals)
//!
//! #### Private Transaction Relaying
//!
//! The relayer allows for submitting proofs for privacy-preserving transactions against the Mixer, Anchor and
//! VAnchor protocols. The users generate zero-knowledge proof data, format a proper payload, and submit
//! it to a compatible relayer for submission.
//!
//! #### Data Querying
//!
//! The relayer also supplements users who need to generate witness data for their zero-knowledge proofs.
//! The relayers cache the leaves of the trees of Mixer, Anchor or VAnchor that they are supporting.
//! This allows users to query for the leaf data faster than querying from a chain directly.
//!
//! #### Data Proposing and Signature Relaying
//!
//! The relayer is tasked with relaying signed data payloads from the DKG's activities and plays an important
//! role as it pertains to the Anchor Protocol. The relayer is responsible for submitting the unsigned and
//! signed anchor update proposals to and from the DKG before and after signing occurs.
//!
//! This role can be divided into two areas:
//! 1. Proposing
//! 2. Relaying
//!
//! The relayer is the main agent in the system who proposes anchor updates to the DKG for signing. That is,
//! the relayer acts as an oracle over the merkle trees of the Anchors and VAnchors. When new insertions into
//! the merkle trees occur, the relayer crafts an update proposal that is eventually proposed to the DKG for signing.
//!
//! The relayer is also responsible for relaying signed proposals. When anchor updates are signed, relayers are
//! tasked with submitting these signed payloads to the smart contract SignatureBridges that verify and handle
//! valid signed proposals. For all other signed proposals, the relayer is tasked with relaying these payloads
//! to the SignatureBridge instances and/or Governable instances.
//!
//! **The responsibility for a relayer to the DKG (governance system) can be summarized as follows:**
//!
//! The relayers act as proposers of proposals intended to be signed by the distributed key generation
//! protocol (DKG).
//!
//!  1. The relayers are listening to and proposing updates.
//!  2. The DKG is signing these updates using a threshold-signature scheme.
//!
//! We require a threshold of relayers (*really proposers*) to agree on the update in order to move the update
//! into a queue for the DKG to sign from.
//!
//! # Features
//!
//! There are several feature flags that control how much is available as part of the crate, both
//! `evm-runtime`, `substrate-runtime` are enabled by default.
//!
//! * `evm-runtime`: Enables the EVM runtime. By default, this is enabled.
//! * `substrate-runtime`: Enables the substrate runtime. By default, this is enabled.
//! * `integration-tests`: Enables integration tests. By default, this is disabled.

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
/// A module for configuring the relayer.
mod config;
/// A module for managing the context of the relayer.
mod context;
/// A module that listens for events on a given chain.
mod events_watcher;
/// A module containing a collection of executable routines.
mod handler;
/// A module used for debugging relayer lifecycle, sync state, or other relayer state.
mod probe;
/// A module containing proposal signing backend (dkg and mocked).
mod proposal_signing_backend;
/// A module for starting long-running tasks for event watching.
mod service;
/// A module for managing the storage of the relayer.
mod store;
/// A module for managing the transaction queue for the relayer.
mod tx_queue;
/// Transaction relaying handlers
mod tx_relay;
/// Types and basic trait implementations for commonly used structs/types
mod types;
/// A module for common functionality.
mod utils;

/// Package identifier, where the default configuration & database are defined.
/// If the user does not start the relayer with the `--config-dir`
/// it will default to read from the default location depending on the OS.
const PACKAGE_ID: [&str; 3] = ["tools", "webb", "webb-relayer"];
/// The Webb Relayer Command-line tool
///
/// Start the relayer from a config file:
///
/// $ webb-relayer -vvv -c <CONFIG_FILE_PATH>
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

    // the build_relayer command sets up routing (endpoint queries / requests mapped to handled code)
    // so clients can interact with the relayer
    let (addr, server) = build_relayer(ctx.clone(), store.clone())?;
    tracing::info!("Starting the server on {}", addr);
    // start the server.
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
/// Sets up the logger for the relayer, based on the verbosity level passed in.
///
/// Returns `Ok(())` on success, or `Err(anyhow::Error)` on failure.
///
/// # Arguments
///
/// * `verbosity` - An i32 integer representing the verbosity level.
///
/// # Examples
///
/// ```
/// let arg = 3;
/// setup_logger(arg)?;
/// ```
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
    let logger = logger.json().flatten_event(true).with_current_span(false);

    logger.init();
    Ok(())
}
/// Loads the configuration from the given directory.
///
/// Returns `Ok(Config)` on success, or `Err(anyhow::Error)` on failure.
///
/// # Arguments
///
/// * `config_dir` - An optional `PathBuf` representing the directory that contains the configuration.
///
/// # Example
///
/// ```
/// let arg = Some(PathBuf::from("/tmp/config"));
/// let config = load_config(arg)?;
/// ```
fn load_config<P>(
    config_dir: Option<P>,
) -> anyhow::Result<config::WebbRelayerConfig>
where
    P: AsRef<Path>,
{
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
/// Sets up the web socket server for the relayer,  routing (endpoint queries / requests mapped to handled code) and
/// instantiates the database store. Allows clients to interact with the relayer.
///
/// Returns `Ok((addr, server))` on success, or `Err(anyhow::Error)` on failure.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `store` - [Sled](https://sled.rs)-based database store
///
/// # Examples
///
/// ```
/// let ctx = RelayerContext::new(config);
/// let store = create_store(&args).await?;
/// let (addr, server) = build_relayer(ctx.clone(), store.clone())?;
/// ```
fn build_relayer(
    ctx: RelayerContext,
    store: store::sled::SledStore,
) -> anyhow::Result<(SocketAddr, impl Future<Output = ()> + 'static)> {
    let port = ctx.config.port;
    let ctx_arc = Arc::new(ctx.clone());
    let ctx_filter = warp::any().map(move || Arc::clone(&ctx_arc)).boxed();

    // the websocket server for users to submit relay transaction requests
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
    // This code identifies the client's ip address and sends it back to them
    // TODO: PUT THE URL FOR THIS ENDPOINT HERE.
    let ip_filter = warp::path("ip")
        .and(warp::get())
        .and(real_ip(vec![proxy_addr]))
        .and_then(handler::handle_ip_info)
        .or(warp::path("ip")
            .and(warp::get())
            .and(warp::addr::remote())
            .and_then(handler::handle_socket_info))
        .boxed();

    // Define the handling of a request for this relayer's information (supported networks)
    // TODO: PUT THE URL FOR THIS ENDPOINT HERE.
    let info_filter = warp::path("info")
        .and(warp::get())
        .and(ctx_filter)
        .and_then(handler::handle_relayer_info)
        .boxed();

    // Define the handling of a request for the leaves of a merkle tree. This is used by clients as a way to query
    // for information needed to generate zero-knowledge proofs (it is faster than querying the chain history)
    // TODO: PUT THE URL FOR THIS ENDPOINT HERE.
    let evm_store = Arc::new(store.clone());
    let store_filter = warp::any().map(move || Arc::clone(&evm_store)).boxed();
    let ctx_arc = Arc::new(ctx.clone());
    let leaves_cache_filter_evm = warp::path("leaves")
        .and(warp::path("evm"))
        .and(store_filter)
        .and(warp::path::param())
        .and(warp::path::param())
        .and_then(move |store, chain_id, contract| {
            handler::handle_leaves_cache_evm(
                store,
                chain_id,
                contract,
                Arc::clone(&ctx_arc),
            )
        })
        .boxed();
    // leaf api handler for substrate
    let substrate_store = Arc::new(store);
    let store_filter = warp::any()
        .map(move || Arc::clone(&substrate_store))
        .boxed();
    let ctx_arc = Arc::new(ctx.clone());

    // TODO: PUT THE URL FOR THIS ENDPOINT HERE.
    let leaves_cache_filter_substrate = warp::path("leaves")
        .and(warp::path("substrate"))
        .and(store_filter)
        .and(warp::path::param())
        .and(warp::path::param())
        .and_then(move |store, chain_id, contract| {
            handler::handle_leaves_cache_substrate(
                store,
                chain_id,
                contract,
                Arc::clone(&ctx_arc),
            )
        })
        .boxed();
    // Code that will map the request handlers above to a defined http endpoint.
    let routes = ip_filter
        .or(info_filter)
        .or(leaves_cache_filter_evm)
        .or(leaves_cache_filter_substrate)
        .boxed(); // will add more routes here.
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
/// Creates a database store for the relayer based on the configuration passed in.
///
/// Returns `Ok(store::sled::SledStore)` on success, or `Err(anyhow::Error)` on failure.
///
/// # Arguments
///
/// * `opts` - The configuration options for the database store.
///
/// # Examples
///
/// ```
/// let args = Args::default();
/// let store = create_store(&args).await?;
/// ```
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
