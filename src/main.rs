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

//! Webb Relayer Binary.
#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use directories_next::ProjectDirs;
use structopt::StructOpt;
use tokio::signal::unix;
//use tokio::{task, time};

use webb_relayer::context::RelayerContext;
use webb_relayer::{config, store};
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

    /*let cloned_store = store.clone();
    let cloned_ctx = ctx.clone();
    // metric for data stored which is determined every 1 hour
    let data_metric_task = tokio::task::spawn(async move {
        let mut sled_data_metric_interval =
            time::interval(Duration::from_secs(3600));

        loop {
            sled_data_metric_interval.tick().await;
            // reset counter first
            cloned_ctx
                .metrics
                .total_number_of_data_stored_metric
                .reset();
            // then get the data stored
            cloned_ctx
                .metrics
                .total_number_of_data_stored_metric
                .inc_by(cloned_store.get_data_stored() as f64)
        }
    });

    data_metric_task.await;*/

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
    let directive_1 = format!("webb_relayer={}", log_level)
        .parse()
        .expect("valid log level");
    let env_filter = tracing_subscriber::EnvFilter::from_default_env()
        .add_directive(directive_1);
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
    config::load(path).map_err(Into::into)
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
async fn create_store(opts: &Opts) -> anyhow::Result<store::SledStore> {
    // check if we shall use the temp dir.
    if opts.tmp {
        tracing::debug!("Using temp dir for store");
        let store = store::SledStore::temporary()?;
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

    let store = store::SledStore::open(db_path)?;
    Ok(store)
}
