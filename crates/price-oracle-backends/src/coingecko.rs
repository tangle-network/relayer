//! Price Backend implementation for `CoinGecko`

use std::sync::Arc;

use futures::TryFutureExt;
use webb_relayer_utils::Result;

/// A backend for fetching prices from `CoinGecko`
#[derive(Clone, Default)]
pub struct CoinGeckoBackend {
    client: Arc<coingecko::CoinGeckoClient>,
}

impl CoinGeckoBackend {
    /// Creates a new `CoinGeckoBackend` with the custom [`coingecko::CoinGeckoClient`]
    #[must_use]
    pub fn with_coin_gecko_client(client: coingecko::CoinGeckoClient) -> Self {
        Self {
            client: Arc::new(client),
        }
    }
}

impl std::fmt::Debug for CoinGeckoBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CoinGeckoBackend")
            .field("client", &"CoinGeckoClient")
            .finish()
    }
}

#[async_trait::async_trait]
impl super::PriceBackend for CoinGeckoBackend {
    async fn get_prices_vs_currency(
        &self,
        tokens: &[&str],
        currency: super::FiatCurrency,
    ) -> Result<super::PricesMap> {
        let prices = self
            .client
            .price(
                tokens,
                &[currency.to_string().to_lowercase()],
                false,
                false,
                false,
                false,
            )
            .map_ok(|m| {
                m.into_iter()
                    .filter_map(|(k, v)| v.usd.map(|price| (k, price)))
                    .collect()
            })
            .await?;
        Ok(prices)
    }
}
