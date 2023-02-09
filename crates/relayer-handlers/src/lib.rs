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

//! Relayer handlers for HTTP/Socket calls

#![allow(clippy::large_enum_variant)]
#![warn(missing_docs)]
use axum::extract::{Path, State, WebSocketUpgrade};
use axum::http::StatusCode;
use ethereum_types::{Address, U256};
use std::error::Error;
use std::sync::Arc;

use futures::prelude::*;

use axum::extract::ws::{Message, WebSocket};
use axum::response::Response;
use axum::Json;
use axum_client_ip::ClientIp;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use webb_proposals::TypedChainId;

use webb_relayer_context::RelayerContext;
use webb_relayer_handler_utils::{
    Command, CommandResponse, CommandStream, CommandType, EvmCommand,
    IpInformationResponse, SubstrateCommand,
};
use webb_relayer_tx_relay::evm::fees::{get_fee_info, FeeInfo};

use crate::routes::HandlerError;
use webb_relayer_tx_relay::evm::vanchor::handle_vanchor_relay_tx;
use webb_relayer_tx_relay::substrate::mixer::handle_substrate_mixer_relay_tx;
use webb_relayer_tx_relay::substrate::vanchor::handle_substrate_vanchor_relay_tx;

/// Module handles relayer API
pub mod routes;

/// Wait for websocket connection upgrade
pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(ctx): State<Arc<RelayerContext>>,
) -> Response {
    ws.on_upgrade(move |socket| accept_websocket_connection(socket, ctx))
}

/// Sets up a websocket connection.
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `stream` - Websocket stream
async fn accept_websocket_connection(ws: WebSocket, ctx: Arc<RelayerContext>) {
    let (mut tx, mut rx) = ws.split();

    // Wait for client to send over text (such as relay transaction requests)
    while let Some(msg) = rx.next().await {
        match msg {
            Ok(msg) => {
                if let Ok(text) = msg.to_text() {
                    // Use inspect_err() here once stabilized
                    let _ =
                        handle_text(&ctx, text, &mut tx).await.map_err(|e| {
                            tracing::warn!("Websocket handler error: {e}")
                        });
                }
            }
            Err(e) => {
                tracing::warn!("Websocket error: {e}");
                return;
            }
        }
    }
}

/// Sets up a websocket channels for message sending.
///
/// This is primarily used for transaction relaying. The intention is
/// that a user will send formatted relay requests to the relayer using
/// the websocket. The command will be extracted and sent to `handle_cmd`
/// if successfully deserialized.
///
/// Returns `Ok(())` on success
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `v` - The text (usually in a JSON form) message to be handled.
/// * `tx` - A mutable Trait implementation of the `warp::ws::Sender` trait
pub async fn handle_text<TX>(
    ctx: &RelayerContext,
    v: &str,
    tx: &mut TX,
) -> webb_relayer_utils::Result<()>
where
    TX: Sink<Message> + Unpin,
    TX::Error: Error + Send + Sync + 'static,
{
    // for every connection, we create a new channel, where we will use to send messages
    // over it.
    let (my_tx, my_rx) = mpsc::channel(50);
    let res_stream = ReceiverStream::new(my_rx);
    match serde_json::from_str(v) {
        Ok(cmd) => {
            if let Err(e) = handle_cmd(ctx.clone(), cmd, my_tx.clone()).await {
                tracing::error!("{:?}", e);
                let _ = my_tx.send(e).await;
            }
            // Send back the response, usually a transaction hash
            // from processing the transaction relaying command.
            res_stream
                .fuse()
                .map(|v| serde_json::to_string(&v).expect("bad value"))
                .inspect(|v| tracing::trace!("Sending: {}", v))
                .map(Message::Text)
                .map(Result::Ok)
                .forward(tx)
                .map_err(|_| webb_relayer_utils::Error::FailedToSendResponse)
                .await?;
        }
        Err(e) => {
            tracing::warn!("Got invalid payload: {:?}", e);
            tracing::debug!("Invalid payload: {:?}", v);
            let error = CommandResponse::Error(e.to_string());
            let value = serde_json::to_string(&error)?;
            tx.send(Message::Text(value))
                .map_err(|_| webb_relayer_utils::Error::FailedToSendResponse)
                .await?;
        }
    };
    Ok(())
}

/// Handles the socket address response
///
/// Returns a Result with the `IpInformationResponse` on success
///
/// # Arguments
///
/// * `ip` - Extractor for client IP, taking into account x-forwarded-for and similar headers
pub async fn handle_socket_info(
    ClientIp(ip): ClientIp,
) -> Json<IpInformationResponse> {
    Json(IpInformationResponse { ip: ip.to_string() })
}

/// Handles the command prompts for EVM and Substrate chains
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `cmd` - The command to execute
/// * `stream` - The stream to write the response to
pub async fn handle_cmd(
    ctx: RelayerContext,
    cmd: Command,
    stream: CommandStream,
) -> Result<(), CommandResponse> {
    use CommandResponse::*;
    if ctx.config.features.private_tx_relay {
        match cmd {
            Command::Substrate(sub) => {
                handle_substrate(ctx, sub, stream).await?
            }
            Command::Evm(evm) => handle_evm(ctx, evm, stream).await?,
            Command::Ping() => {
                let _ = stream.send(Pong()).await;
            }
        }
    } else {
        return Err(Error(
            "Private transaction relaying is not enabled.".to_string(),
        ));
    }
    Ok(())
}

/// Handler for EVM commands
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `cmd` - The command to execute
/// * `stream` - The stream to write the response to
pub async fn handle_evm(
    ctx: RelayerContext,
    cmd: EvmCommand,
    stream: CommandStream,
) -> Result<(), CommandResponse> {
    if let CommandType::VAnchor(_) = cmd {
        handle_vanchor_relay_tx(ctx, cmd, stream.clone()).await?;
    }
    Ok(())
}

/// Handler for Substrate commands
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `cmd` - The command to execute
/// * `stream` - The stream to write the response to
pub async fn handle_substrate<'a>(
    ctx: RelayerContext,
    cmd: SubstrateCommand,
    stream: CommandStream,
) -> Result<(), CommandResponse> {
    match cmd {
        CommandType::Mixer(_) => {
            handle_substrate_mixer_relay_tx(ctx, cmd, stream).await?;
        }
        CommandType::VAnchor(_) => {
            handle_substrate_vanchor_relay_tx(ctx, cmd, stream).await?;
        }
    }
    Ok(())
}

/// Handler for fee estimation
///
/// # Arguments
///
/// * `chain_id` - ID of the blockchain
/// * `vanchor` - Address of the smart contract
/// * `gas_amount` - How much gas the transaction needs. Don't use U256 here because it
///                  gets parsed incorrectly.
pub async fn handle_fee_info(
    State(ctx): State<Arc<RelayerContext>>,
    Path((chain_id, vanchor, gas_amount)): Path<(u64, Address, u64)>,
) -> Result<Json<FeeInfo>, HandlerError> {
    let chain_id = TypedChainId::from(chain_id);
    let gas_amount = U256::from(gas_amount);
    get_fee_info(chain_id, vanchor, gas_amount, ctx.as_ref())
        .await
        .map(Json)
        .map_err(|e| {
            HandlerError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
        })
}
