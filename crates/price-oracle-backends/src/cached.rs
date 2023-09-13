use std::{collections::HashSet, time::Duration};

use chrono::{DateTime, NaiveDateTime, Utc};
use webb_relayer_store::TokenPriceCacheStore;
use webb_relayer_utils::Result;

/// A price backend that caches the price data in a local database
///
/// The cache is used to reduce the number of requests to the source and to improve the performance.
///
/// **Note:** depending on the configuration, this backend may be used to return the last saved price
/// data even if the source is unavailable, which may lead to incorrect price data.
#[derive(Debug, Clone, typed_builder::TypedBuilder)]
pub struct CachedPriceBackend<B, S> {
    /// The price backend
    backend: B,
    /// The local data store used for caching
    store: S,
    /// The cache expiration time.
    ///
    /// If the cache is older than this value, it will be refreshed
    /// from the source backend.
    ///
    /// If the value is `None`, the cache will never expire
    /// and will never be refreshed. **This may lead to incorrect price data.**
    ///
    /// Use this option only if you are sure that the source backend is always available.
    /// Otherwise, use a reasonable value, The default value is `15 minutes`.
    /// see [`Self::use_cache_if_source_unavailable`] and [`Self::even_if_expired`]
    /// for fine tuning the cache behavior.
    #[builder(default = Some(Duration::from_secs(15 * 60)))]
    cache_expiration: Option<Duration>,
    /// Specifies whether the cache should be returned even if the source is unavailable
    ///
    /// If the value is `true`, the cache will be returned even if the source
    /// backend is unavailable unless the cache is expired.
    ///
    /// see [`Self::even_if_expired`] if you want to return the cache even if it is expired.
    #[builder(setter(strip_bool))]
    use_cache_if_source_unavailable: bool,
    /// Specifies whether the cache should be returned even if it is expired
    /// in case the source is unavailable.
    ///
    /// see [`Self::use_cache_if_source_unavailable`] if you want to return the cache
    /// even if the source is unavailable.
    #[builder(setter(strip_bool))]
    even_if_expired: bool,
}

/// A cached price data
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct CachedPrice {
    /// The Cached Price of the token
    pub price: f64,
    /// The timestamp of the cached price
    pub timestamp: i64,
}

impl<B, S> CachedPriceBackend<B, S>
where
    B: super::PriceBackend,
    S: TokenPriceCacheStore<CachedPrice>,
{
    /// Returns the cache expiration duration
    pub const fn cache_expiration(&self) -> Option<Duration> {
        self.cache_expiration
    }

    /// Returns `true` if the cache should be returned even if the source is unavailable
    pub const fn use_cache_if_source_unavailable_enabled(&self) -> bool {
        self.use_cache_if_source_unavailable
    }

    /// Returns the inner price backend
    pub const fn inner(&self) -> &B {
        &self.backend
    }

    /// Returns the inner data store
    /// The data store is used for caching the price data
    pub const fn store(&self) -> &S {
        &self.store
    }
}

#[async_trait::async_trait]
impl<B, S> super::PriceBackend for CachedPriceBackend<B, S>
where
    B: super::PriceBackend + Clone + 'static,
    S: TokenPriceCacheStore<CachedPrice> + Clone + Send + Sync + 'static,
{
    async fn get_prices_vs_currency(
        &self,
        tokens: &[&str],
        vs_currency: super::FiatCurrency,
    ) -> Result<super::PricesMap> {
        // The returned prices map
        let mut prices = super::PricesMap::new();
        // The tokens that need to be fetched from the source
        let mut tokens_to_fetch = HashSet::new();

        for token in tokens {
            let token_key = format!("{token}/{vs_currency}");
            // Check if the token is cached
            if let Some(cached) = self.store.get_price(&token_key)? {
                let expired =
                    self.cache_expiration.map_or(false, |expiration| {
                        let ts = NaiveDateTime::from_timestamp_opt(
                            cached.timestamp + expiration.as_secs() as i64,
                            Default::default(),
                        )
                        .expect("Time went backwards");
                        DateTime::<Utc>::from_naive_utc_and_offset(ts, Utc) < Utc::now()
                    });
                // If the cache is expired, add the token to the list of tokens to fetch
                if expired {
                    tokens_to_fetch.insert(token.to_owned());
                } else {
                    prices.insert((*token).to_owned(), cached.price);
                }
            } else {
                // If the token is not cached, add it to the list of tokens to fetch
                tokens_to_fetch.insert(token.to_owned());
            }
        }
        if !tokens_to_fetch.is_empty() {
            // Fetch the prices from the source
            let token_ids = tokens_to_fetch.iter().copied().collect::<Vec<_>>();
            let result = self
                .backend
                .get_prices_vs_currency(&token_ids, vs_currency)
                .await;
            let source_unavailable = result.is_err();
            let updated_prices = match result {
                Ok(updated_prices) => updated_prices,
                Err(err) => {
                    // If the source is unavailable and the cache is enabled, return the cache
                    if self.use_cache_if_source_unavailable {
                        super::PricesMap::new()
                    } else {
                        return Err(err);
                    }
                }
            };

            // If the source is unavailable and the cache is enabled and `even_if_expired` is enabled,
            // return the cache
            if source_unavailable
                && self.use_cache_if_source_unavailable
                && self.even_if_expired
            {
                // refetch the cache, and ignore the expiration
                for token in tokens {
                    let token_key = format!("{token}/{vs_currency}");
                    if let Some(cached) = self.store.get_price(&token_key)? {
                        prices.insert((*token).to_owned(), cached.price);
                    }
                }
            }

            // Update the cache, only if the source is available
            let source_available = !source_unavailable;
            if source_available {
                for (token, price) in updated_prices {
                    let token_key = format!("{token}/{vs_currency}");
                    prices.insert(token.clone(), price);
                    self.store.insert_price(
                        &token_key,
                        CachedPrice {
                            price,
                            timestamp: Utc::now().timestamp(),
                        },
                    )?;
                }
            }
        }
        Ok(prices)
    }
}

#[cfg(test)]
mod tests {
    use crate::PriceBackend;

    use super::*;

    fn make_backend() -> crate::DummyPriceBackend {
        let prices = crate::PricesMap::from_iter([
            (String::from("tTNT"), 0.10),
            (String::from("WETH"), 1000.0),
            (String::from("USDC"), 1.0),
        ]);
        crate::DummyPriceBackend::new(prices)
    }

    fn make_store() -> webb_relayer_store::InMemoryStore {
        webb_relayer_store::InMemoryStore::default()
    }

    #[tokio::test]
    async fn it_works() {
        let backend = CachedPriceBackend::builder()
            .backend(make_backend())
            .store(make_store())
            .build();
        let prices = backend.get_prices(&["USDC"]).await.unwrap();
        assert_eq!(prices.len(), 1);
        assert_eq!(prices.get("USDC"), Some(&1.0));
    }
}
