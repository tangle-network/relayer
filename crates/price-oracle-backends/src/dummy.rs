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
        let result = self
            .prices
            .iter()
            .filter(|(token, _)| tokens.contains(&token.as_str()))
            .map(|(token, price)| (token.clone(), *price))
            .collect();
        Ok(result)
    }
}
