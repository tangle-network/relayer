use jsonrpsee::core::client::{
    Client, ClientT, SubscriptionClientT, SubscriptionKind,
};
use jsonrpsee::core::traits::ToRpcParams;
use jsonrpsee::types::SubscriptionId;
use jsonrpsee::ws_client::WsClientBuilder;
use serde_json::value::RawValue;
use tangle_subxt::subxt::backend::rpc::{
    RawRpcFuture, RawRpcSubscription, RpcClientT,
};
use tangle_subxt::subxt::error::RpcError;
use tangle_subxt::subxt::ext::futures::{StreamExt, TryStreamExt};

#[derive(Debug)]
pub struct WebbRpcClient(pub Client);

struct Params(Option<Box<RawValue>>);
impl ToRpcParams for Params {
    fn to_rpc_params(
        self,
    ) -> Result<Option<Box<RawValue>>, jsonrpsee::core::Error> {
        Ok(self.0)
    }
}

impl WebbRpcClient {
    pub async fn new(
        url: impl Into<String>,
    ) -> webb_relayer_utils::Result<Self> {
        let url: http::Uri = url.into().parse().map_err(|_| {
            webb_relayer_utils::Error::Generic("RPC url is invalid")
        })?;
        let client = WsClientBuilder::default()
            .build(url.to_string())
            .await
            .map_err(|_| {
                webb_relayer_utils::Error::Generic("RPC failed to connect")
            })?;

        Ok(Self(client))
    }
}

impl RpcClientT for WebbRpcClient {
    fn request_raw<'a>(
        &'a self,
        method: &'a str,
        params: Option<Box<RawValue>>,
    ) -> RawRpcFuture<'a, Box<RawValue>> {
        Box::pin(async move {
            let res = self
                .0
                .request(method, Params(params))
                .await
                .map_err(|e| RpcError::ClientError(Box::new(e)))?;

            Ok(res)
        })
    }

    fn subscribe_raw<'a>(
        &'a self,
        sub: &'a str,
        params: Option<Box<RawValue>>,
        unsub: &'a str,
    ) -> RawRpcFuture<'a, RawRpcSubscription> {
        Box::pin(async move {
            let stream = SubscriptionClientT::subscribe::<Box<RawValue>, _>(
                &self.0,
                sub,
                Params(params),
                unsub,
            )
            .await
            .map_err(|e| RpcError::ClientError(Box::new(e)))?;

            let id = match stream.kind() {
                SubscriptionKind::Subscription(SubscriptionId::Str(id)) => {
                    Some(id.clone().into_owned())
                }
                _ => None,
            };

            let stream = stream
                .map_err(|e| RpcError::ClientError(Box::new(e)))
                .boxed();
            Ok(RawRpcSubscription { stream, id })
        })
    }
}
