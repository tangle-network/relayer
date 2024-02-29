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
//! ```rust,ignore
//! use webb_price_oracle_backends::{CoinGeckoBackend, PriceBackend};
//! let backend = CoinGeckoBackend::builder().build();
//! let prices = backend.get_prices(&["ETH", "BTC"]).await.unwrap();
//! let eth_price = prices.get("ETH").unwrap();
//! let btc_price = prices.get("BTC").unwrap();
//! ```
//! ## With Cache
//! ```rust,ignore
//! use price_oracle_backends::{CoinGeckoBackend, PriceBackend, CachedPriceBackend};
//! use webb_relayer_store::SledStore;
//! let store = SledStore::temporary()?;
//! let backend = CoinGeckoBackend::builder().build();
//! let backend = CachedPriceBackend::new(backend, store);
//! let prices = backend.get_prices(&["ETH", "BTC"]).await.unwrap();
//! let eth_price = prices.get("ETH").unwrap();
//! let btc_price = prices.get("BTC").unwrap();
//! ```
//!
//! ## Features flags
//! - `coingecko` - enables `CoinGecko` backend

#![deny(unsafe_code)]
#![warn(missing_docs)]

use std::{fmt::Display, sync::Arc};

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

#[async_trait::async_trait]
impl<O> PriceBackend for Arc<O>
where
    O: PriceBackend + ?Sized,
{
    async fn get_prices(&self, tokens: &[&str]) -> Result<PricesMap> {
        PriceBackend::get_prices_vs_currency(
            self.as_ref(),
            tokens,
            FiatCurrency::default(),
        )
        .await
    }

    async fn get_prices_vs_currency(
        &self,
        tokens: &[&str],
        vs_currency: FiatCurrency,
    ) -> Result<PricesMap> {
        PriceBackend::get_prices_vs_currency(self.as_ref(), tokens, vs_currency)
            .await
    }
}
