use webb_relayer_utils::Result;

/// A Dummy Price Oracle Backend
///
/// This backend is useful for testing purposes, it always returns the same price data
/// that is configured initially while creating the backend.
#[derive(Debug, Clone)]
pub struct DummyPriceBackend {
    /// The price data that is returned by the backend
    prices: super::PricesMap,
}

impl DummyPriceBackend {
    /// Creates a new dummy price backend
    #[must_use]
    pub fn new(prices: super::PricesMap) -> Self {
        Self { prices }
    }
}

#[async_trait::async_trait]
impl super::PriceBackend for DummyPriceBackend {
    async fn get_prices_vs_currency(
        &self,
        tokens: &[&str],
        _currency: super::FiatCurrency,
    ) -> Result<super::PricesMap> {
        let result = tokens
            .iter()
            .copied()
            .filter_map(|token| {
                self.prices
                    .get(token)
                    .copied()
                    .map(|price| (token.to_owned(), price))
            })
            .collect();
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use crate::PriceBackend;

    use super::*;

    #[tokio::test]
    async fn it_works() {
        let backend = DummyPriceBackend::new(
            vec![("ETH".to_string(), 100.0), ("DOT".to_string(), 10.0)]
                .into_iter()
                .collect(),
        );
        let prices = backend.get_prices(&["ETH", "DOT"]).await.unwrap();
        assert_eq!(prices.len(), 2);
        assert_eq!(prices["ETH"], 100.0);
        assert_eq!(prices["DOT"], 10.0);
    }

    #[tokio::test]
    async fn non_existing_tokens() {
        let backend = DummyPriceBackend::new(
            vec![("ETH".to_string(), 100.0), ("DOT".to_string(), 10.0)]
                .into_iter()
                .collect(),
        );
        let prices = backend.get_prices(&["ETH", "DOT", "KSM"]).await.unwrap();
        assert_eq!(prices.len(), 2);
        assert_eq!(prices["ETH"], 100.0);
        assert_eq!(prices["DOT"], 10.0);
    }
}
