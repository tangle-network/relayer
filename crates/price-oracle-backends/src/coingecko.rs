//! Price Backend implementation for `CoinGecko`

use std::{collections::HashMap, sync::Arc};

use futures::TryFutureExt;
use serde::de::DeserializeOwned;
use webb_relayer_utils::Result;

/// A backend for fetching prices from `CoinGecko`
#[derive(Clone, Debug, typed_builder::TypedBuilder)]
pub struct CoinGeckoBackend {
    #[builder(
        default = String::from("https://api.coingecko.com/api/v3"),
        setter(into)
    )]
    host: String,
    #[builder(default = Arc::new(reqwest::Client::new()))]
    client: Arc<reqwest::Client>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SimplePriceResponse {
    pub(crate) usd: Option<f64>,
}

impl CoinGeckoBackend {
    async fn get<R: DeserializeOwned>(&self, endpoint: &str) -> Result<R> {
        let url = format!("{}/{}", self.host, endpoint);
        self.client
            .get(&url)
            .send()
            .await?
            .json()
            .await
            .map_err(Into::into)
    }

    async fn price<Id: AsRef<str>, Curr: AsRef<str>>(
        &self,
        ids: &[Id],
        vs_currencies: &[Curr],
    ) -> Result<HashMap<String, SimplePriceResponse>> {
        let ids = ids.iter().map(AsRef::as_ref).collect::<Vec<_>>();
        let vs_currencies =
            vs_currencies.iter().map(AsRef::as_ref).collect::<Vec<_>>();
        let req = format!(
            "simple/price?ids={}&vs_currencies={}",
            ids.join("%2C"),
            vs_currencies.join("%2C")
        );
        self.get(&req).await
    }
}

#[async_trait::async_trait]
impl super::PriceBackend for CoinGeckoBackend {
    async fn get_prices_vs_currency(
        &self,
        tokens: &[&str],
        currency: super::FiatCurrency,
    ) -> Result<super::PricesMap> {
        // map token names to coingecko ids
        let mut id_to_token = HashMap::new();
        let chains_info = webb_chains_info::chains_info();
        for token in tokens {
            let id = chains_info
                .iter()
                .find_map(|(_, info)| {
                    info.native_currency
                        .symbol
                        .eq(*token)
                        .then_some(info.native_currency.coingecko_coin_id)
                })
                .flatten()
                .unwrap_or(*token);
            id_to_token.insert(id, *token);
        }
        let ids = id_to_token.keys().collect::<Vec<_>>();

        let prices: crate::PricesMap = self
            .price(&ids, &[currency.to_string().to_lowercase()])
            .map_ok(|m| {
                m.into_iter()
                    .filter_map(|(k, v)| v.usd.map(|price| (k, price)))
                    .collect()
            })
            .await?;
        // remap the ids back to token names
        let prices = prices
            .into_iter()
            .filter_map(|(id, price)| {
                id_to_token
                    .get(id.as_str())
                    .map(|t| ((*t).to_string(), price))
            })
            .collect();
        Ok(prices)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::PriceBackend;

    use super::*;
    use coingecko_mocked_server::*;

    mod coingecko_mocked_server {
        use axum::extract::{Query, State};
        use axum::response::IntoResponse;
        use axum::{response::Json, routing::get, Router};
        use std::collections::HashMap;
        use std::net::SocketAddr;
        use std::sync::atomic::Ordering;
        use std::{
            sync::{atomic::AtomicBool, Arc},
            time::Duration,
        };

        fn random_free_port() -> u16 {
            std::net::TcpListener::bind("127.0.0.1:0")
                .unwrap()
                .local_addr()
                .unwrap()
                .port()
        }

        #[derive(Debug, Clone, typed_builder::TypedBuilder)]
        pub struct MockServer {
            hard_coded_prices: crate::PricesMap,
            #[builder(
                default = Arc::new(Duration::from_secs(0)),
                setter(transform = |d: Duration| Arc::new(d))
            )]
            simulated_delay: Arc<std::time::Duration>,
            #[builder(
                default,
                setter(transform = |b: bool| Arc::new(AtomicBool::new(b))),
            )]
            simulat_server_error: Arc<AtomicBool>,
        }

        pub struct MockedServerHandle {
            simulat_server_error: Arc<AtomicBool>,
            backend: super::CoinGeckoBackend,
            server_thread: tokio::task::JoinHandle<()>,
        }

        impl Drop for MockedServerHandle {
            fn drop(&mut self) {
                self.simulat_server_error.store(true, Ordering::Relaxed);
                self.server_thread.abort();
            }
        }

        impl MockedServerHandle {
            pub fn backend(&self) -> super::CoinGeckoBackend {
                self.backend.clone()
            }

            /// Simulate a server error, this will cause all requests to fail
            pub fn simulate_server_error(&self, v: bool) {
                self.simulat_server_error.store(v, Ordering::Relaxed);
            }
        }

        #[derive(serde::Deserialize)]
        struct RequestQuery {
            ids: String,
            #[allow(dead_code)]
            vs_currencies: String,
        }

        #[derive(Clone)]
        struct MockState {
            hard_coded_prices: crate::PricesMap,
            simulated_delay: Arc<std::time::Duration>,
            simulat_server_error: Arc<AtomicBool>,
        }

        async fn prices_handler(
            Query(query): Query<RequestQuery>,
            State(mock_state): State<MockState>,
        ) -> impl IntoResponse {
            if mock_state.simulat_server_error.load(Ordering::Relaxed) {
                return Err(Json("Simulated Server Error"));
            }

            let mut prices = HashMap::new();
            for token in query.ids.split(',') {
                if let Some(price) = mock_state.hard_coded_prices.get(token) {
                    prices.insert(
                        token.to_string(),
                        super::SimplePriceResponse { usd: Some(*price) },
                    );
                }
            }

            tokio::time::sleep(*mock_state.simulated_delay).await;
            Ok(Json(prices))
        }

        impl MockServer {
            pub fn spwan(self) -> MockedServerHandle {
                let simulat_server_error = self.simulat_server_error.clone();
                let port = random_free_port();
                let addr = SocketAddr::from(([127, 0, 0, 1], port));
                let url = format!("http://{addr}/api/v3");
                let backend =
                    super::CoinGeckoBackend::builder().host(url).build();
                let handle = tokio::spawn(async move {
                    let api_v3 = Router::new()
                        .route("/simple/price", get(prices_handler));
                    let app = Router::new()
                        .nest("/api/v3/", api_v3)
                        .with_state(MockState {
                            hard_coded_prices: self.hard_coded_prices,
                            simulated_delay: self.simulated_delay.clone(),
                            simulat_server_error: self
                                .simulat_server_error
                                .clone(),
                        });

                    axum::Server::bind(&addr)
                        .serve(app.into_make_service())
                        .await
                        .unwrap();
                });

                MockedServerHandle {
                    simulat_server_error,
                    backend,
                    server_thread: handle,
                }
            }
        }
    }

    fn build_hard_coded_prices() -> crate::PricesMap {
        let mut prices = crate::PricesMap::new();
        prices.insert(String::from("ethereum"), 1000.0);
        prices.insert(String::from("matic-network"), 1.0);
        prices
    }

    #[tokio::test]
    async fn it_works() {
        let mock_server = MockServer::builder()
            .hard_coded_prices(build_hard_coded_prices())
            .build();
        let handle = mock_server.spwan();
        // Wait for the server to start
        tokio::time::sleep(Duration::from_millis(100)).await;
        let backend = handle.backend();
        let prices = backend.get_prices(&["ETH", "MATIC"]).await.unwrap();
        assert_eq!(prices.get("ETH"), Some(&1000.0));
        assert_eq!(prices.get("MATIC"), Some(&1.0));
    }

    #[tokio::test]
    async fn fails_when_server_errors() {
        let mock_server = MockServer::builder()
            .hard_coded_prices(build_hard_coded_prices())
            .build();
        let handle = mock_server.spwan();
        // Wait for the server to start
        tokio::time::sleep(Duration::from_millis(100)).await;
        let backend = handle.backend();
        handle.simulate_server_error(true);
        let prices = backend.get_prices(&["ETH", "MATIC"]).await;
        assert!(prices.is_err());
    }

    #[tokio::test]
    async fn should_keep_working_if_cached() {
        let mock_server = MockServer::builder()
            .hard_coded_prices(build_hard_coded_prices())
            .build();
        let handle = mock_server.spwan();
        // Wait for the server to start
        tokio::time::sleep(Duration::from_millis(100)).await;
        let backend = handle.backend();
        let cached_backend = crate::CachedPriceBackend::builder()
            .backend(backend)
            .store(webb_relayer_store::InMemoryStore::default())
            .use_cache_if_source_unavailable()
            // Disable cache expiration
            .cache_expiration(None)
            .build();
        let prices =
            cached_backend.get_prices(&["ETH", "MATIC"]).await.unwrap();
        assert_eq!(prices.get("ETH"), Some(&1000.0));
        assert_eq!(prices.get("MATIC"), Some(&1.0));

        // Simulate a server error
        handle.simulate_server_error(true);
        // The cache should still work
        let prices =
            cached_backend.get_prices(&["ETH", "MATIC"]).await.unwrap();
        assert_eq!(prices.get("ETH"), Some(&1000.0));
        assert_eq!(prices.get("MATIC"), Some(&1.0));
    }

    #[tokio::test]
    async fn should_not_work_if_the_cache_expired() {
        let mock_server = MockServer::builder()
            .hard_coded_prices(build_hard_coded_prices())
            .build();
        let handle = mock_server.spwan();
        // Wait for the server to start
        tokio::time::sleep(Duration::from_millis(100)).await;
        let backend = handle.backend();
        let cached_backend = crate::CachedPriceBackend::builder()
            .backend(backend)
            .store(webb_relayer_store::InMemoryStore::default())
            .cache_expiration(Some(Duration::from_secs(2)))
            .use_cache_if_source_unavailable()
            .build();
        let prices =
            cached_backend.get_prices(&["ETH", "MATIC"]).await.unwrap();
        assert_eq!(prices.get("ETH"), Some(&1000.0));
        assert_eq!(prices.get("MATIC"), Some(&1.0));

        // Simulate a server error
        handle.simulate_server_error(true);
        // The cache should still work
        let prices =
            cached_backend.get_prices(&["ETH", "MATIC"]).await.unwrap();
        assert_eq!(prices.get("ETH"), Some(&1000.0));
        assert_eq!(prices.get("MATIC"), Some(&1.0));

        // Wait for the cache to expire
        tokio::time::sleep(Duration::from_secs(2)).await;
        // The cache should not work
        let prices =
            cached_backend.get_prices(&["ETH", "MATIC"]).await.unwrap();
        assert_eq!(prices.get("ETH"), None);
        assert_eq!(prices.get("MATIC"), None);
    }

    #[tokio::test]
    async fn should_keep_working_if_cache_expired() {
        let mock_server = MockServer::builder()
            .hard_coded_prices(build_hard_coded_prices())
            .build();
        let handle = mock_server.spwan();
        // Wait for the server to start
        tokio::time::sleep(Duration::from_millis(100)).await;
        let backend = handle.backend();
        let cached_backend = crate::CachedPriceBackend::builder()
            .backend(backend)
            .store(webb_relayer_store::InMemoryStore::default())
            .cache_expiration(Some(Duration::from_secs(2)))
            .use_cache_if_source_unavailable()
            .even_if_expired()
            .build();
        let prices =
            cached_backend.get_prices(&["ETH", "MATIC"]).await.unwrap();
        assert_eq!(prices.get("ETH"), Some(&1000.0));
        assert_eq!(prices.get("MATIC"), Some(&1.0));

        // Simulate a server error
        handle.simulate_server_error(true);
        // The cache should still work
        let prices =
            cached_backend.get_prices(&["ETH", "MATIC"]).await.unwrap();
        assert_eq!(prices.get("ETH"), Some(&1000.0));
        assert_eq!(prices.get("MATIC"), Some(&1.0));

        // Wait for the cache to expire
        tokio::time::sleep(Duration::from_secs(2)).await;
        // The cache should still work
        let prices =
            cached_backend.get_prices(&["ETH", "MATIC"]).await.unwrap();
        assert_eq!(prices.get("ETH"), Some(&1000.0));
        assert_eq!(prices.get("MATIC"), Some(&1.0));
    }

    #[tokio::test]
    async fn should_fail_if_token_not_listed_or_mapped() {
        let mock_server = MockServer::builder()
            .hard_coded_prices(build_hard_coded_prices())
            .build();
        let handle = mock_server.spwan();
        // Wait for the server to start
        tokio::time::sleep(Duration::from_millis(100)).await;
        let backend = handle.backend();
        let prices = backend.get_prices(&["BTC"]).await.unwrap();
        assert!(prices.is_empty());
    }
}
