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
//! let eth_price = prices.get("ETH").unwrap();
//! let btc_price = prices.get("BTC").unwrap();
//! ```
//! ## With Cache
//! ```rust,no_run
//! use price_oracle_backends::{CoinGeckoBackend, PriceBackend, CachedPriceBackend};
//! use webb_relayer_store::SledStore;
//! let store = SledStore::temporary()?;
//! let backend = CoinGeckoBackend::new();
//! let backend = CachedPriceBackend::new(backend, store);
//! let prices = backend.get_prices(&["ETH", "BTC"]).await?;
//! let eth_price = prices.get("ETH").unwrap();
//! let btc_price = prices.get("BTC").unwrap();
//! ```
//!
//! ## Features flags
//! - `coingecko` - enables `CoinGecko` backend

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::fmt::Display;

use webb_relayer_utils::Result;

/// Cached Backend Module
mod cached;
/// `CoinGecko` Backend
#[cfg(feature = "coingecko")]
mod coingecko;
/// A Dymmy Price Backend
mod dummy;
/// Merger Backend Module
mod merger;

#[cfg(feature = "coingecko")]
pub use crate::coingecko::CoinGeckoBackend;
pub use cached::CachedPriceBackend;
pub use dummy::DummyPriceBackend;
pub use merger::PriceOracleMerger;

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
    #[must_use]
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
    async fn get_prices(&self, tokens: &[&str]) -> Result<PricesMap> {
        PriceBackend::get_prices_vs_currency(
            self,
            tokens,
            FiatCurrency::default(),
        )
        .await
    }
    /// Returns the prices for the given tokens in the requested currency
    async fn get_prices_vs_currency(
        &self,
        tokens: &[&str],
        vs_currency: FiatCurrency,
    ) -> Result<PricesMap>;
}
