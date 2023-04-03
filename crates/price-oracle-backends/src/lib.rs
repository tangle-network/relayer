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

//! Price Oracle Backends
//!
//! A Price Oracle Backend is a service that provides price data for a list of tokens symbols in
//! the requested currency. The backend is responsible for fetching the price data from the source
//! and converting it to the requested currency.
//!
//! The backend may optionally cache the price data in a local database. The cache is used to
//! reduce the number of requests to the source and to improve the performance of the backend.
//!
//! As of now, the following backends are supported:
//! - [CoinGecko](https://www.coingecko.com/en/api)
//!
//! ## Usage
//! ```rust,no_run
//! use price_oracle_backends::{CoinGeckoBackend, PriceBackend};
//! let backend = CoinGeckoBackend::new();
//! let prices = backend.get_prices(&["ETH", "BTC"]).await?;
//! let eth_price = prices.get("ETH").expect("ETH price is missing");
//! let btc_price = prices.get("BTC").expect("BTC price is missing");
//! ```
//! ## With Cache
//! ```rust,no_run
//! use price_oracle_backends::{CoinGeckoBackend, PriceBackend, CachedPriceBackend};
//! use webb_relayer_store::SledStore;
//! let store = SledStore::temporary()?;
//! let backend = CoinGeckoBackend::new();
//! let backend = CachedPriceBackend::new(backend, store);
//! let prices = backend.get_prices(&["ETH", "BTC"]).await?;
//! let eth_price = prices.get("ETH").expect("ETH price is missing");
//! let btc_price = prices.get("BTC").expect("BTC price is missing");
//! ```
//!
//! ## Features flags
//! - `coingecko` - enables CoinGecko backend

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::collections::HashSet;
use std::fmt::Display;
use std::time::Duration;

use chrono::{DateTime, NaiveDateTime, Utc};
use webb_relayer_store::TokenPriceCacheStore;
use webb_relayer_utils::Result;

#[cfg(feature = "coingecko")]
pub mod coingecko;
#[cfg(feature = "coingecko")]
pub use crate::coingecko::CoinGeckoBackend;

/// A List of supported fiat currencies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FiatCurrency {
    /// United States Dollar
    ///
    /// The default currency
    #[default]
    USD,
    /// Euro
    EUR,
    /// British Pound
    GBP,
}

impl FiatCurrency {
    /// Returns the currency symbol
    pub fn symbol(&self) -> &'static str {
        match self {
            Self::USD => "$",
            Self::EUR => "€",
            Self::GBP => "£",
        }
    }
}

impl Display for FiatCurrency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::USD => write!(f, "USD"),
            Self::EUR => write!(f, "EUR"),
            Self::GBP => write!(f, "GBP"),
        }
    }
}

/// A type alias for a map of token symbols to prices
type PricesMap = std::collections::HashMap<String, f64>;

/// A trait for a price backend
#[async_trait::async_trait]
pub trait PriceBackend: Send + Sync {
    /// Returns the prices for the given tokens in the USD currency
    ///
    /// This is a convenience method that calls `get_prices_vs_currency` with the default currency
    async fn get_prices<T>(&self, tokens: T) -> Result<PricesMap>
    where
        T: IntoIterator + Send + Sync,
        T::Item: AsRef<str>,
    {
        PriceBackend::get_prices_vs_currency(
            self,
            tokens,
            FiatCurrency::default(),
        )
        .await
    }
    /// Returns the prices for the given tokens in the requested currency
    async fn get_prices_vs_currency<T>(
        &self,
        tokens: T,
        vs_currency: FiatCurrency,
    ) -> Result<PricesMap>
    where
        T: IntoIterator + Send + Sync,
        T::Item: AsRef<str>;
}

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
    /// The cache expiration time in seconds
    /// If the cache is older than this value, it will be refreshed
    /// If the value is `None`, the cache will never expire
    /// The default value is `15 minutes`.
    #[builder(default = Some(Duration::from_secs(15 * 60)), setter(strip_option))]
    cache_expiration: Option<Duration>,
    /// Specifies whether the cache should be returned even if the source is unavailable
    ///
    /// If the value is `true`, the cache will be returned even if the source
    /// backend is unavailable even if the cache is expired.
    #[builder(setter(strip_bool))]
    use_cache_if_source_unavailable: bool,
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
    B: PriceBackend,
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
impl<B, S> PriceBackend for CachedPriceBackend<B, S>
where
    B: PriceBackend + Clone + Send + Sync + 'static,
    S: TokenPriceCacheStore<CachedPrice> + Clone + Send + Sync + 'static,
{
    async fn get_prices_vs_currency<T>(
        &self,
        tokens: T,
        vs_currency: FiatCurrency,
    ) -> Result<PricesMap>
    where
        T: IntoIterator + Send + Sync,
        T::Item: AsRef<str>,
    {
        // The returned prices map
        let mut prices = PricesMap::new();
        // The tokens that need to be fetched from the source
        let mut tokens_to_fetch = HashSet::new();

        for token in tokens {
            let token_key = format!("{}/{}", token.as_ref(), vs_currency);
            // Check if the token is cached
            if let Some(cached) = self.store.get_price(&token_key)? {
                let expired = self
                    .cache_expiration
                    .map(|expiration| {
                        let ts = NaiveDateTime::from_timestamp_opt(
                            cached.timestamp + expiration.as_secs() as i64,
                            Default::default(),
                        )
                        .expect("Time went backwards");
                        DateTime::<Utc>::from_utc(ts, Utc) < Utc::now()
                    })
                    .unwrap_or(false);
                // If the cache is expired, add the token to the list of tokens to fetch
                if expired {
                    tokens_to_fetch.insert(token.as_ref().to_owned());
                } else {
                    prices.insert(token_key, cached.price);
                }
            } else {
                // If the token is not cached, add it to the list of tokens to fetch
                tokens_to_fetch.insert(token.as_ref().to_owned());
            }
        }
        if !tokens_to_fetch.is_empty() {
            // Fetch the prices from the source
            let result = self
                .backend
                .get_prices_vs_currency(&tokens_to_fetch, vs_currency)
                .await;
            let updated_prices = match result {
                Ok(updated_prices) => updated_prices,
                Err(err) => {
                    // If the source is unavailable and the cache is enabled, return the cache
                    if self.use_cache_if_source_unavailable {
                        PricesMap::new()
                    } else {
                        return Err(err);
                    }
                }
            };

            // Update the cache
            for (token, price) in updated_prices {
                let token_key = format!("{}/{}", token, vs_currency);
                prices.insert(token_key.clone(), price);
                self.store.insert_price(
                    &token_key,
                    CachedPrice {
                        price,
                        timestamp: Utc::now().timestamp(),
                    },
                )?;
            }
        }
        Ok(prices)
    }
}
