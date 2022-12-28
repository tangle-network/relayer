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
use std::convert::Infallible;
use std::error::Error;
use std::net::{IpAddr, SocketAddr};

use futures::prelude::*;

use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use warp::ws::Message;

use webb_relayer_context::RelayerContext;
use webb_relayer_handler_utils::{
    Command, CommandResponse, CommandStream, CommandType, EvmCommand,
    IpInformationResponse, SubstrateCommand,
};

use webb_relayer_tx_relay::evm::vanchor::handle_vanchor_relay_tx;
use webb_relayer_tx_relay::substrate::mixer::handle_substrate_mixer_relay_tx;
use webb_relayer_tx_relay::substrate::vanchor::handle_substrate_vanchor_relay_tx;

/// Module handles relayer API
pub mod routes;

/// Sets up a websocket connection.
///
/// Returns `Ok(())` on success
///
/// # Arguments
///
/// * `ctx` - RelayContext reference that holds the configuration
/// * `stream` - Websocket stream
pub async fn accept_connection(
    ctx: &RelayerContext,
    stream: warp::ws::WebSocket,
) -> webb_relayer_utils::Result<()> {
    let (mut tx, mut rx) = stream.split();

    // Wait for client to send over text (such as relay transaction requests)
    while let Some(msg) = rx.try_next().await? {
        if let Ok(text) = msg.to_str() {
            handle_text(ctx, text, &mut tx).await?;
        }
    }
    Ok(())
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
            handle_cmd(ctx.clone(), cmd, my_tx).await;
            // Send back the response, usually a transaction hash
            // from processing the transaction relaying command.
            res_stream
                .fuse()
                .map(|v| serde_json::to_string(&v).expect("bad value"))
                .inspect(|v| tracing::trace!("Sending: {}", v))
                .map(Message::text)
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
            tx.send(Message::text(value))
                .map_err(|_| webb_relayer_utils::Error::FailedToSendResponse)
                .await?;
        }
    };
    Ok(())
}

/// Handles the `ip` address response
///
/// Returns a Result with the `IpInformationResponse` on success
///
/// # Arguments
///
/// * `ip` - Option containing the IP address
pub async fn handle_ip_info(
    ip: Option<IpAddr>,
) -> Result<impl warp::Reply, Infallible> {
    Ok(warp::reply::json(&IpInformationResponse {
        ip: ip.unwrap().to_string(),
    }))
}
/// Handles the socket address response
///
/// Returns a Result with the `IpInformationResponse` on success
///
/// # Arguments
///
/// * `ip` - Option containing the socket address
pub async fn handle_socket_info(
    ip: Option<SocketAddr>,
) -> Result<impl warp::Reply, Infallible> {
    Ok(warp::reply::json(&IpInformationResponse {
        ip: ip.unwrap().ip().to_string(),
    }))
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
) {
    use CommandResponse::*;
    if ctx.config.features.private_tx_relay {
        match cmd {
            Command::Substrate(sub) => handle_substrate(ctx, sub, stream).await,
            Command::Evm(evm) => handle_evm(ctx, evm, stream).await,
            Command::Ping() => {
                let _ = stream.send(Pong()).await;
            }
        }
    } else {
        tracing::error!("Private transaction relaying is not configured..!");
        let _ = stream
            .send(Error(
                "Private transaction relaying is not enabled.".to_string(),
            ))
            .await;
    }
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
) {
    if let CommandType::VAnchor(_) = cmd {
        handle_vanchor_relay_tx(ctx, cmd, stream).await
    }
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
) {
    match cmd {
        CommandType::Mixer(_) => {
            handle_substrate_mixer_relay_tx(ctx, cmd, stream).await;
        }
        CommandType::VAnchor(_) => {
            handle_substrate_vanchor_relay_tx(ctx, cmd, stream).await;
        }
    }
}
