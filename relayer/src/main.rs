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
        .merge(config::Environment::with_prefix("WEBB"))?;
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
pub struct RelayerWithdrawProof {
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

impl From<RelayerWithdrawProof> for mixer::WithdrawProof<WebbRuntime> {
    fn from(p: RelayerWithdrawProof) -> Self {
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

impl From<mixer::WithdrawProof<WebbRuntime>> for RelayerWithdrawProof {
    fn from(p: mixer::WithdrawProof<WebbRuntime>) -> Self {
        Self {
            mixer_id: p.mixer_id,
            cached_block: p.cached_block,
            cached_root: p.cached_root.0,
            comms: p.comms.into_iter().map(|v| v.0).collect(),
            nullifier_hash: p.nullifier_hash.0,
            proof_bytes: p.proof_bytes,
            leaf_index_commitments: p
                .leaf_index_commitments
                .into_iter()
                .map(|v| v.0)
                .collect(),
            proof_commitments: p
                .proof_commitments
                .into_iter()
                .map(|v| v.0)
                .collect(),
            recipient: p.recipient.map(|v| *v.as_ref()),
            relayer: p.relayer.map(|v| *v.as_ref()),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Command {
    Withdraw(RelayerWithdrawProof),
    Ping(),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum WithdrawStatus {
    Sent,
    Submitted,
    Finlized,
    Errored { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
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
                yield Withdraw(Sent);
                let result = ctx.client.withdraw_and_watch(&ctx.pair, proof.into()).await;
                yield Withdraw(Submitted);
                match result {
                    Ok(_) => {
                        yield Withdraw(Finlized);
                    },
                    Err(e) => yield Withdraw(Errored { reason: e.to_string() }),
                }
            }
        }
    }
}

#[cfg(all(test, feature = "integration-tests"))]
mod tests {
    use super::*;
    use bulletproofs::r1cs::Prover;
    use bulletproofs::{BulletproofGens, PedersenGens};
    use bulletproofs_gadgets::fixed_deposit_tree::builder::{
        FixedDepositTree, FixedDepositTreeBuilder,
    };
    use bulletproofs_gadgets::poseidon::builder::Poseidon;
    use bulletproofs_gadgets::poseidon::{PoseidonBuilder, PoseidonSbox};
    use curve25519_dalek::scalar::Scalar;
    use merlin::Transcript;
    use runtime::pallet::merkle::*;
    use sp_keyring::AccountKeyring;

    type CachedRoots = CachedRootsStore<WebbRuntime>;
    type Leaves = LeavesStore<WebbRuntime>;

    // Default hasher instance used to construct the tree
    fn default_hasher() -> Poseidon {
        let width = 6;
        // TODO: should be able to pass the number of generators
        let bp_gens = BulletproofGens::new(16400, 1);
        PoseidonBuilder::new(width)
            .bulletproof_gens(bp_gens)
            .sbox(PoseidonSbox::Exponentiation3)
            .build()
    }
    async fn get_client() -> Client<WebbRuntime> {
        ClientBuilder::new()
            .set_url("ws://127.0.0.1:9944")
            .build()
            .await
            .unwrap()
    }

    fn generate_proof(
        tree: &mut FixedDepositTree,
        mixer_id: u32,
        cached_block: u32,
        leaf: [u8; 32],
        root: [u8; 32],
        recipient: AccountId32,
        relayer: AccountId32,
    ) -> RelayerWithdrawProof {
        let pc_gens = PedersenGens::default();
        let bp_gens = BulletproofGens::new(16400, 1);
        let mut prover_transcript = Transcript::new(b"zk_membership_proof");
        let prover = Prover::new(&pc_gens, &mut prover_transcript);

        let root = Scalar::from_bytes_mod_order(root);
        let leaf = Scalar::from_bytes_mod_order(leaf);
        let recipient = Scalar::from_bytes_mod_order(*recipient.as_ref());
        let relayer = Scalar::from_bytes_mod_order(*relayer.as_ref());
        let (
            proof_bytes,
            (comms, nullifier_hash, leaf_index_commitments, proof_commitments),
        ) = tree.prove_zk(root, leaf, recipient, relayer, &bp_gens, prover);

        let comms = comms
            .into_iter()
            .map(|v| Commitment(v.to_bytes()))
            .collect();
        let leaf_index_commitments = leaf_index_commitments
            .into_iter()
            .map(|v| Commitment(v.to_bytes()))
            .collect();
        let proof_commitments = proof_commitments
            .into_iter()
            .map(|v| Commitment(v.to_bytes()))
            .collect();
        let nullifier_hash = ScalarData(nullifier_hash.to_bytes());
        let proof_bytes = proof_bytes.to_bytes();
        let recipient = ScalarData(recipient.to_bytes());
        let relayer = ScalarData(relayer.to_bytes());
        let proof: mixer::WithdrawProof<WebbRuntime> = mixer::WithdrawProof {
            relayer: Some(AccountId32::from(relayer.0)),
            recipient: Some(AccountId32::from(recipient.0)),
            proof_bytes,
            nullifier_hash,
            proof_commitments,
            leaf_index_commitments,
            comms,
            mixer_id,
            cached_root: ScalarData(root.to_bytes()),
            cached_block,
        };
        proof.into()
    }

    #[tokio::test]
    async fn relay() {
        let mut tree = FixedDepositTreeBuilder::new()
            .hash_params(default_hasher())
            .depth(32)
            .build();
        let tree_id = 0;
        let leaf = tree.generate_secrets();
        let client = get_client().await;
        let pair = PairSigner::new(AccountKeyring::Alice.pair());
        let result = client
            .deposit_and_watch(
                &pair,
                tree_id,
                vec![ScalarData(leaf.to_bytes())],
            )
            .await;
        let xt = result.unwrap();
        println!("Hash: {:?}", xt.block);
        let maybe_block = client.block(Some(xt.block)).await.unwrap();
        let signed_block = maybe_block.unwrap();
        println!("Number: #{}", signed_block.block.header.number);
        let leaves = {
            let mut leaves = vec![];
            for i in 0..1024 {
                let maybe_leaf = client
                    .fetch(&Leaves::try_get(tree_id, i), None)
                    .await
                    .unwrap();
                match maybe_leaf {
                    Some(leaf) if leaf.0 != [0u8; 32] => leaves.push(leaf.0),
                    _ => continue,
                };
            }
            leaves
        };
        let cached_roots = client
            .fetch(&CachedRoots::new(signed_block.block.header.number, 0), None)
            .await
            .unwrap();
        let roots = cached_roots.unwrap();
        let root = roots[0];
        tree.tree.add_leaves(leaves, Some(root.0));

        let recipient = AccountKeyring::Charlie.to_account_id();
        let relayer = AccountKeyring::Bob.to_account_id();

        let proof = generate_proof(
            &mut tree,
            tree_id,
            signed_block.block.header.number,
            leaf.to_bytes(),
            root.0,
            recipient,
            relayer,
        );
        let ctx = RelayerContext {
            client: client.clone(),
            pair: PairSigner::new(AccountKeyring::Bob.pair()),
        };

        let event_stream = handle_cmd(ctx, Command::Withdraw(proof));
        futures::pin_mut!(event_stream);

        let event = event_stream.next().await;
        dbg!(&event);
        assert_eq!(
            event,
            Some(CommandResponse::Withdraw(WithdrawStatus::Sent))
        );

        let event = event_stream.next().await;
        dbg!(&event);
        assert_eq!(
            event,
            Some(CommandResponse::Withdraw(WithdrawStatus::Submitted))
        );

        let event = event_stream.next().await;
        dbg!(&event);
        assert_eq!(
            event,
            Some(CommandResponse::Withdraw(WithdrawStatus::Finlized))
        );
    }
}
