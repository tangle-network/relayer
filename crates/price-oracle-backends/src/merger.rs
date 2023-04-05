use webb_relayer_utils::Result;

/// A Price Oracle Merger backend is a backend that builds on top of other backends and merges the
/// price data from the underlying backends. The merger backend is useful when you want to use
/// multiple backends to fetch the price data and merge the results. For example, you can use the
/// `CoinGecko` backend to fetch the price data and the `CoinMarketCap` backend to fetch the price
/// data for the tokens that are not supported by the `CoinGecko` backend.
///
/// ## Semantics
///
/// The merger backend will fetch the price data from the underlying backends and merge the results,
/// the following rules are applied:
/// - If the price data is available in all backends, the price data from the **last** merged backend is used.
/// - If the price is not available in any of the backends, the price data is not included in the result.
#[allow(clippy::module_name_repetitions)]
pub struct PriceOracleMerger {
    /// The underlying backends
    backends: Vec<Box<dyn super::PriceBackend>>,
}

impl PriceOracleMerger {
    /// Creates a new `PriceOracleMergerBuilder`
    #[must_use]
    pub fn builder() -> PriceOracleMergerBuilder {
        PriceOracleMergerBuilder {
            backends: Vec::default(),
        }
    }
}

/// A builder for the `PriceOracleMerger`
pub struct PriceOracleMergerBuilder {
    backends: Vec<Box<dyn super::PriceBackend>>,
}

impl PriceOracleMergerBuilder {
    /// Merges the price data from the underlying backends
    ///
    /// The price data is merged according to the following rules:
    /// - If the price data is available in all backends, the price data from the **last** merged backend is used.
    /// - If the price is not available in any of the backends, the price data is not included in the result.
    #[must_use]
    pub fn merge(mut self, backend: Box<dyn super::PriceBackend>) -> Self {
        self.backends.push(backend);
        self
    }

    /// Builds the `PriceOracleMerger`
    #[must_use]
    pub fn build(self) -> PriceOracleMerger {
        PriceOracleMerger {
            backends: self.backends,
        }
    }
}

impl PriceOracleMerger {
    /// Merges the price data from the underlying backends
    ///
    /// The price data is merged according to the following rules:
    /// - If the price data is available in all backends, the price data from the **last** merged backend is used.
    /// - If the price is not available in any of the backends, the price data is not included in the result.
    pub fn merge(
        &mut self,
        backend: Box<dyn super::PriceBackend>,
    ) -> &mut Self {
        self.backends.push(backend);
        self
    }
}

#[async_trait::async_trait]
impl super::PriceBackend for PriceOracleMerger {
    async fn get_prices_vs_currency(
        &self,
        tokens: &[&str],
        currency: super::FiatCurrency,
    ) -> Result<super::PricesMap> {
        let mut prices = super::PricesMap::new();
        for backend in &self.backends {
            let backend_prices =
                backend.get_prices_vs_currency(tokens, currency).await?;
            prices.extend(backend_prices);
        }
        Ok(prices)
    }
}
