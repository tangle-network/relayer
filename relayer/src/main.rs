use std::error::Error;

use async_stream::stream;
use futures::prelude::*;
use futures::SinkExt;
use runtime::pallet::{mixer, Commitment, ScalarData};
use runtime::subxt::sp_core::crypto::AccountId32;
use runtime::WebbRuntime;
use serde::{Deserialize, Serialize};
use tokio::net::{TcpListener, TcpStream};
use tungstenite::tokio::accept_async_with_config;
use tungstenite::tungstenite::protocol::WebSocketConfig;
use tungstenite::tungstenite::Message;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::try_init_from_env("WEBB_LOG")?;
    let socket = TcpListener::bind("0.0.0.0:9933").await?;
    println!("Starting on port 9933");
    while let Ok((stream, _)) = socket.accept().await {
        log::debug!("Client Connected: {}", stream.peer_addr()?);
        tokio::spawn(accept_connection(stream));
    }
    Ok(())
}

async fn accept_connection(stream: TcpStream) -> anyhow::Result<()> {
    let config = WebSocketConfig {
        max_send_queue: Some(5),
        max_message_size: Some(2 << 20), // 2MB
        ..Default::default()
    };
    let ws_stream = accept_async_with_config(stream, Some(config)).await?;
    let (mut tx, mut rx) = ws_stream.split();
    while let Some(msg) = rx.try_next().await? {
        match msg {
            Message::Text(v) => handle_text(v, &mut tx).await?,
            Message::Binary(_) => {
                // should we close the connection?
            },
            _ => continue,
        }
    }
    Ok(())
}

async fn handle_text<TX>(v: String, tx: &mut TX) -> anyhow::Result<()>
where
    TX: Sink<Message> + Unpin,
    TX::Error: Error + Send + Sync + 'static,
{
    match serde_json::from_str(&v) {
        Ok(cmd) => {
            handle_cmd(cmd)
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
    Test(),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
enum CommandResponse {
    Ok(),
    Error(String),
}

fn handle_cmd(cmd: Command) -> impl Stream<Item = CommandResponse> {
    stream! {
        match cmd {
            Command::Test() => yield CommandResponse::Ok(),
            Command::Withdraw(proof) => {
                dbg!(&proof);
                let _proof: mixer::WithdrawProof<WebbRuntime> = proof.into();
                // TODO: submit the proof to a transaction
            }
        }
    }
}
