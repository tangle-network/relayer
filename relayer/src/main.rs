use std::error::Error;
use std::path::PathBuf;

use anyhow::Context;
use async_stream::stream;
use directories_next::ProjectDirs;
use futures::prelude::*;
use futures::SinkExt;
use runtime::pallet::mixer::*;
use runtime::pallet::{mixer, Commitment, ScalarData};
use runtime::subxt::sp_core::crypto::{AccountId32, Pair};
use runtime::subxt::sp_core::sr25519;
use runtime::subxt::{Client, ClientBuilder, PairSigner};
use runtime::WebbRuntime;
use serde::{Deserialize, Serialize};
use structopt::StructOpt;
use tokio::net::{TcpListener, TcpStream};
use tungstenite::tokio::accept_async_with_config;
use tungstenite::tungstenite::protocol::WebSocketConfig;
use tungstenite::tungstenite::Message;

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

const fn default_port() -> u16 { 9955 }

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct WebbRelayerConfig {
    /// WebSocket Server Port number
    ///
    /// default to 9955
    #[serde(default = "default_port")]
    pub port: u16,
    /// Interprets the string in order to generate a key Pair.
    ///
    /// - If `s` is a possibly `0x` prefixed 64-digit hex string, then it will
    ///   be interpreted
    /// directly as a `MiniSecretKey` (aka "seed" in `subkey`).
    /// - If `s` is a valid BIP-39 key phrase of 12, 15, 18, 21 or 24 words,
    ///   then the key will
    /// be derived from it. In this case:
    ///   - the phrase may be followed by one or more items delimited by `/`
    ///     characters.
    ///   - the path may be followed by `///`, in which case everything after
    ///     the `///` is treated
    /// as a password.
    /// - If `s` begins with a `/` character it is prefixed with the Substrate
    ///   public `DEV_PHRASE` and
    /// interpreted as above.
    ///
    /// In this case they are interpreted as HDKD junctions; purely numeric
    /// items are interpreted as integers, non-numeric items as strings.
    /// Junctions prefixed with `/` are interpreted as soft junctions, and
    /// with `//` as hard junctions.
    ///
    /// There is no correspondence mapping between SURI strings and the keys
    /// they represent. Two different non-identical strings can actually
    /// lead to the same secret being derived. Notably, integer junction
    /// indices may be legally prefixed with arbitrary number of zeros.
    /// Similarly an empty password (ending the SURI with `///`) is perfectly
    /// valid and will generally be equivalent to no password at all.
    pub suri: String,
}

fn load<P: Into<PathBuf>>(path: P) -> anyhow::Result<WebbRelayerConfig> {
    let base: PathBuf = path.into();
    let mut cfg = config::Config::new();
    cfg.merge(config::File::with_name(&base.display().to_string()))?
        .merge(config::Environment::with_prefix("APP"))?;
    cfg.try_into().map_err(Into::into)
}

#[derive(Clone)]
struct RelayerContext {
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
    let config = load(config_path).context("failed to load the config file")?;
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
        tokio::spawn(accept_connection(ctx.clone(), stream));
    }
    Ok(())
}

async fn accept_connection(
    ctx: RelayerContext,
    stream: TcpStream,
) -> anyhow::Result<()> {
    let config = WebSocketConfig {
        max_send_queue: Some(5),
        max_message_size: Some(2 << 20), // 2MB
        ..Default::default()
    };
    let ws_stream = accept_async_with_config(stream, Some(config)).await?;
    let (mut tx, mut rx) = ws_stream.split();
    while let Some(msg) = rx.try_next().await? {
        match msg {
            Message::Text(v) => handle_text(&ctx, v, &mut tx).await?,
            Message::Binary(_) => {
                // should we close the connection?
            },
            _ => continue,
        }
    }
    Ok(())
}

async fn handle_text<TX>(
    ctx: &RelayerContext,
    v: String,
    tx: &mut TX,
) -> anyhow::Result<()>
where
    TX: Sink<Message> + Unpin,
    TX::Error: Error + Send + Sync + 'static,
{
    match serde_json::from_str(&v) {
        Ok(cmd) => {
            handle_cmd(ctx.clone(), cmd)
                .map(|v| serde_json::to_string(&v).expect("bad value"))
                .map(Message::Text)
                .map(Result::Ok)
                .forward(tx)
                .await?;
        },
        Err(e) => {
            let error = CommandResponse::Error(e.to_string());
            let value = serde_json::to_string(&error)?;
            tx.send(Message::Text(value)).await?
        },
    };
    Ok(())
}

/// Proof data for withdrawal
#[derive(Debug, PartialEq, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawProof {
    /// The mixer id this withdraw proof corresponds to
    pub mixer_id: u32,
    /// The cached block for the cached root being proven against
    pub cached_block: u32,
    /// The cached root being proven against
    pub cached_root: [u8; 32],
    /// The individual scalar commitments (to the randomness and nullifier)
    pub comms: Vec<[u8; 32]>,
    /// The nullifier hash with itself
    pub nullifier_hash: [u8; 32],
    /// The proof in bytes representation
    pub proof_bytes: Vec<u8>,
    /// The leaf index scalar commitments to decide on which side to hash
    pub leaf_index_commitments: Vec<[u8; 32]>,
    /// The scalar commitments to merkle proof path elements
    pub proof_commitments: Vec<[u8; 32]>,
    /// The recipient to withdraw amount of currency to
    pub recipient: Option<[u8; 32]>,
    /// The recipient to withdraw amount of currency to
    pub relayer: Option<[u8; 32]>,
}

impl From<WithdrawProof> for mixer::WithdrawProof<WebbRuntime> {
    fn from(p: WithdrawProof) -> Self {
        Self {
            mixer_id: p.mixer_id,
            cached_block: p.cached_block,
            cached_root: ScalarData(p.cached_root),
            comms: p.comms.into_iter().map(Commitment).collect(),
            nullifier_hash: ScalarData(p.nullifier_hash),
            proof_bytes: p.proof_bytes,
            leaf_index_commitments: p
                .leaf_index_commitments
                .into_iter()
                .map(Commitment)
                .collect(),
            proof_commitments: p
                .proof_commitments
                .into_iter()
                .map(Commitment)
                .collect(),
            recipient: p.recipient.map(AccountId32::new),
            relayer: p.relayer.map(AccountId32::new),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Command {
    Withdraw(WithdrawProof),
    Ping(),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
enum WithdrawStatus {
    Sent,
    Submitted,
    Finlized(u32),
    Errored { reason: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
enum CommandResponse {
    Pong(),
    Withdraw(WithdrawStatus),
    Error(String),
}

fn handle_cmd(
    ctx: RelayerContext,
    cmd: Command,
) -> impl Stream<Item = CommandResponse> {
    use CommandResponse::*;
    stream! {
        match cmd {
            Command::Ping() => yield Pong(),
            Command::Withdraw(proof) => {
                use WithdrawStatus::*;
                let proof = mixer::WithdrawProof::from(proof);
                yield Withdraw(Sent);
                let result = ctx.client.withdraw_and_watch(&ctx.pair, proof).await;
                dbg!(&result);
                yield Withdraw(Submitted);
                match result {
                    Ok(xt) => {
                        dbg!(xt);
                        yield Pong();
                        yield Withdraw(Finlized(10));
                    },
                    Err(e) => yield Withdraw(Errored { reason: e.to_string() }),
                }
            }
        }
    }
}
