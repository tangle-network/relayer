use crate::WebbRelayerConfig;
use anyhow::Context;
use directories_next::ProjectDirs;
use std::path::{Path, PathBuf};
use structopt::StructOpt;

/// Package identifier, where the default configuration & database are defined.
/// If the user does not start the relayer with the `--config-dir`
/// it will default to read from the default location depending on the OS.
pub const PACKAGE_ID: [&str; 3] = ["tools", "webb", "webb-relayer"];

/// The Webb Relayer Command-line tool
///
/// Start the relayer from a config file:
///
/// $ webb-relayer -vvv -c <CONFIG_FILE_PATH>
#[derive(StructOpt)]
#[structopt(name = "Webb Relayer")]
pub struct Opts {
    /// A level of verbosity, and can be used multiple times
    #[allow(dead_code)]
    #[structopt(short, long, parse(from_occurrences))]
    pub verbose: i32,
    /// Directory that contains configration files.
    #[structopt(
        short = "c",
        long = "config-dir",
        value_name = "PATH",
        parse(from_os_str)
    )]
    pub config_dir: Option<PathBuf>,
    /// Create the Database Store in a temporary directory.
    /// and will be deleted when the process exits.
    #[structopt(long)]
    pub tmp: bool,
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
pub fn load_config<P>(
    config_dir: Option<P>,
) -> Result<WebbRelayerConfig, anyhow::Error>
where
    P: AsRef<Path>,
{
    tracing::debug!("Getting default dirs for webb relayer");
    let dirs = ProjectDirs::from(PACKAGE_ID[0], PACKAGE_ID[1], PACKAGE_ID[2])
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
    let v = crate::utils::load(path)?;
    tracing::trace!("Config loaded..");
    Ok(v)
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
pub fn setup_logger(verbosity: i32) -> anyhow::Result<()> {
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
pub async fn create_store(
    opts: &Opts,
) -> anyhow::Result<webb_relayer_store::SledStore> {
    // check if we shall use the temp dir.
    if opts.tmp {
        tracing::debug!("Using temp dir for store");
        let store = webb_relayer_store::SledStore::temporary()?;
        return Ok(store);
    }
    let dirs = ProjectDirs::from(PACKAGE_ID[0], PACKAGE_ID[1], PACKAGE_ID[2])
        .context("failed to get config")?;
    let p = match opts.config_dir.as_ref() {
        Some(p) => p.to_path_buf(),
        None => dirs.data_local_dir().to_path_buf(),
    };
    let db_path = match opts.config_dir.as_ref().zip(p.parent()) {
        Some((_, parent)) => parent.join("store"),
        None => p.join("store"),
    };

    let store = webb_relayer_store::SledStore::open(db_path)?;
    Ok(store)
}
