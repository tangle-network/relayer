use std::sync::Arc;

pub use jsonrpsee::core::client::Client;
use webb::substrate::subxt::{self, rpc::RpcClientT};

#[derive(Debug, Clone)]
pub struct WebbRpcClient(pub Arc<jsonrpsee::async_client::Client>);

impl WebbRpcClient {
    pub async fn new(
        url: impl Into<String>,
    ) -> webb_relayer_utils::Result<Self> {
        let url: http::Uri = url.into().parse().map_err(|_| {
            webb_relayer_utils::Error::Generic("RPC url is invalid")
        })?;
        let (sender, receiver) =
            jsonrpsee::client_transport::ws::WsTransportClientBuilder::default(
            )
            .build(url)
            .await
            .map_err(|_| {
                webb_relayer_utils::Error::Generic("RPC failed to connect")
            })?;

        let client = jsonrpsee::async_client::ClientBuilder::default()
            .max_notifs_per_subscription(4096)
            .build_with_tokio(sender, receiver);

        Ok(Self(Arc::new(client)))
    }
}

impl RpcClientT for WebbRpcClient {
    fn request_raw<'a>(
        &'a self,
        method: &'a str,
        params: Option<Box<jsonrpsee::core::JsonRawValue>>,
    ) -> subxt::rpc::RpcFuture<'a, Box<jsonrpsee::core::JsonRawValue>> {
        self.0.request_raw(method, params)
    }

    fn subscribe_raw<'a>(
        &'a self,
        sub: &'a str,
        params: Option<Box<jsonrpsee::core::JsonRawValue>>,
        unsub: &'a str,
    ) -> subxt::rpc::RpcFuture<'a, subxt::rpc::RpcSubscription> {
        self.0.subscribe_raw(sub, params, unsub)
    }
}
