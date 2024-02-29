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

use crate::Error as WebbRelayerError;
use core::fmt::Debug;
use futures::prelude::*;
use serde::{de::DeserializeOwned, Serialize};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use webb::evm::ethers::providers::{JsonRpcClient, ProviderError};
/// MultiProvider is a JsonRpcClient that will round-robin requests to the underlying providers.
#[derive(Debug, Clone)]
pub struct MultiProvider<P> {
    providers: Arc<Vec<P>>,
    last_used: Arc<AtomicUsize>,
}

impl<P> MultiProvider<P> {
    pub fn new(providers: Arc<Vec<P>>) -> Self {
        Self {
            providers,
            last_used: Default::default(),
        }
    }
}

#[async_trait::async_trait]
impl<P: JsonRpcClient> JsonRpcClient for MultiProvider<P>
where
    P::Error: Into<ProviderError>,
{
    type Error = ProviderError;

    async fn request<
        T: Debug + Serialize + Send + Sync,
        R: DeserializeOwned + Send,
    >(
        &self,
        method: &str,
        params: T,
    ) -> Result<R, Self::Error> {
        // Fetch the next provider index to use
        // incrementing it by 1 and wrapping around if it exceeds the number of providers
        let next_provider_idx = self
            .last_used
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |last_used| {
                Some(last_used.saturating_add(1) % self.providers.len())
            })
            .unwrap_or_default();

        if let Some(provider) = self.providers.get(next_provider_idx) {
            provider
                .request(method, params)
                .map_err(P::Error::into)
                .await
        } else {
            Err(ProviderError::CustomError(
                WebbRelayerError::ProviderNotFound(next_provider_idx)
                    .to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;
    use webb::evm::ethers::providers::{self, Http, Middleware};

    #[tokio::test]
    async fn should_process_request() {
        let p1 = Http::from_str("https://eth.llamarpc.com").unwrap();
        let p2 = Http::from_str("https://1rpc.io/eth").unwrap();

        let multi_provider = MultiProvider::new(vec![p1, p2].into());
        assert_eq!(multi_provider.providers.len(), 2);
        assert_eq!(multi_provider.last_used.load(Ordering::SeqCst), 0);
        let provider = providers::Provider::new(multi_provider.clone());
        provider.get_block_number().await.expect("should work");
        assert_eq!(multi_provider.last_used.load(Ordering::SeqCst), 1);
        provider.get_block_number().await.expect("should work");
    }
}
