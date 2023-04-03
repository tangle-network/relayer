//! Price Backend implementation for CoinGecko

use std::sync::Arc;

use futures::TryFutureExt;
use webb_relayer_utils::Result;

/// A backend for fetching prices from CoinGecko
#[derive(Clone, Default)]
pub struct CoinGeckoBackend {
    client: Arc<coingecko::CoinGeckoClient>,
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
    async fn get_prices_vs_currency<T>(
        &self,
        tokens: T,
        currency: super::FiatCurrency,
    ) -> Result<super::PricesMap>
    where
        T: IntoIterator + Send + Sync,
        T::Item: AsRef<str>,
    {
        let token_ids = tokens
            .into_iter()
            .map(|token| token.as_ref().to_owned())
            .collect::<Vec<_>>();
        let prices = self
            .client
            .price(
                &token_ids,
                &[currency.to_string()],
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
