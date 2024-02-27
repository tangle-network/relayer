// Copyright (C) 2022-2024 Webb Technologies Inc.
//
// Tangle is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Tangle is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should receive a copy of the GNU General Public License
// If not, see <http://www.gnu.org/licenses/>.

use jsonrpsee::async_client::ClientBuilder;
use jsonrpsee::client_transport::ws::WsTransportClientBuilder;
use jsonrpsee::core::client::Client;
use jsonrpsee::core::JsonRawValue;
use webb::substrate::subxt::{self, rpc::RpcClientT};

#[derive(Debug)]
pub struct WebbRpcClient(pub Client);

impl WebbRpcClient {
    pub async fn new(
        url: impl Into<String>,
    ) -> webb_relayer_utils::Result<Self> {
        let url: http::Uri = url.into().parse().map_err(|_| {
            webb_relayer_utils::Error::Generic("RPC url is invalid")
        })?;
        let (sender, receiver) = WsTransportClientBuilder::default()
            .build(url)
            .await
            .map_err(|_| {
                webb_relayer_utils::Error::Generic("RPC failed to connect")
            })?;

        let client = ClientBuilder::default()
            .max_notifs_per_subscription(4096)
            .build_with_tokio(sender, receiver);

        Ok(Self(client))
    }
}

impl RpcClientT for WebbRpcClient {
    fn request_raw<'a>(
        &'a self,
        method: &'a str,
        params: Option<Box<JsonRawValue>>,
    ) -> subxt::rpc::RpcFuture<'a, Box<JsonRawValue>> {
        self.0.request_raw(method, params)
    }

    fn subscribe_raw<'a>(
        &'a self,
        sub: &'a str,
        params: Option<Box<JsonRawValue>>,
        unsub: &'a str,
    ) -> subxt::rpc::RpcFuture<'a, subxt::rpc::RpcSubscription> {
        self.0.subscribe_raw(sub, params, unsub)
    }
}
