use coingecko::CoinGeckoClient;
use ethers::etherscan;
use ethers::types::Chain;

/// Pull USD prices of wrapped token and base token from coingecko.com, and use these to
/// calculate the exchange rate.
pub async fn calculate_exchange_rate(
    wrapped_token: &str,
    base_token: &str,
) -> f64 {
    // TODO: relatively heavyweight as it pulls in reqwest. would be better for build time
    //       to make a simple http request, if we dont need other coingecko functionality
    let client = CoinGeckoClient::default();
    let prices = client
        .price(
            &[wrapped_token, base_token],
            &["usd"],
            false,
            false,
            false,
            false,
        )
        .await
        .unwrap();
    let wrapped_price = prices[wrapped_token].usd.unwrap();
    let base_price = prices[base_token].usd.unwrap();
    wrapped_price / base_price
}

/// Estimate gas price using etherscan.io. Note that this functionality is only available
/// on mainnet.
pub async fn estimate_gas_price() -> crate::Result<u64> {
    // fee estimation using etherscan, only supports mainnet
    let client = etherscan::Client::builder()
        // TODO: need to add actual api key via config
        .with_api_key("YourApiKeyToken")
        .chain(Chain::Mainnet)
        .unwrap()
        .build()
        .unwrap();
    // using "average" gas price
    Ok(client.gas_oracle().await.unwrap().propose_gas_price)
}
