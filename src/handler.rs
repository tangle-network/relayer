use std::error::Error;

use async_stream::stream;
use futures::prelude::*;
use webb::pallet::mixer::{self, *};
use webb::pallet::*;
use webb::substrate::subxt::sp_runtime::AccountId32;
use webb::substrate::WebbRuntime;
use serde::{Deserialize, Serialize};
use tokio::net::TcpStream;
use tungstenite::tokio::accept_async_with_config;
use tungstenite::tungstenite::protocol::WebSocketConfig;
use tungstenite::tungstenite::Message;

use super::RelayerContext;

pub async fn accept_connection(
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

pub async fn handle_text<TX>(
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
pub enum Command {
    Withdraw(RelayerWithdrawProof),
    Ping(),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WithdrawStatus {
    Sent,
    Submitted,
    Finlized,
    Errored { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CommandResponse {
    Pong(),
    Withdraw(WithdrawStatus),
    Error(String),
}

pub fn handle_cmd(
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
