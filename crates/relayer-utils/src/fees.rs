use coingecko::CoinGeckoClient;
use ethers::etherscan;
use ethers::types::Chain;
use once_cell::sync::Lazy;
use webb::evm::ethers::prelude::U256;

/// Maximum refund amount per relay transaction in USD.
const MAX_REFUND_USD: f64 = 1.;

/// Number of digits after the comma of USD-Coin.
const USDC_DECIMALS: u32 = 6;

static COIN_GECKO_CLIENT: Lazy<CoinGeckoClient> =
    Lazy::new(CoinGeckoClient::default);

static ETHERSCAN_CLIENT: Lazy<etherscan::Client> = Lazy::new(|| {
    etherscan::Client::builder()
        // TODO: Need to add actual api key via config to increase rate limit
        .with_api_key("YourApiKeyToken")
        .chain(Chain::Mainnet)
        .unwrap()
        .build()
        .unwrap()
});

/// Calculate fee in `wrappedToken`, using the estimated gas price from etherscan.
pub async fn calculate_wrapped_fee(
    estimated_gas_amount: U256,
    exchange_rate: f64,
) -> U256 {
    let gas_price = U256::from(estimate_gas_price().await.unwrap());
    let native_fee = (gas_price * estimated_gas_amount).as_u128() as f64;
    let wrapped_fee = native_fee * exchange_rate;
    to_u256(wrapped_fee)
}

/// Pull USD prices of wrapped token and base token from coingecko.com, and use these to
/// calculate the exchange rate.
pub async fn calculate_exchange_rate(
    wrapped_token: &str,
    base_token: &str,
) -> f64 {
    let tokens = &[wrapped_token, base_token];
    let prices = COIN_GECKO_CLIENT
        .price(tokens, &["usd"], false, false, false, false)
        .await
        .unwrap();
    let wrapped_price = prices[wrapped_token].usd.unwrap();
    let base_price = prices[base_token].usd.unwrap();
    wrapped_price / base_price
}

/// Estimate gas price using etherscan.io. Note that this functionality is only available
/// on mainnet.
async fn estimate_gas_price() -> crate::Result<u64> {
    let gas_oracle = ETHERSCAN_CLIENT.gas_oracle().await.unwrap();
    // use the "average" gas price
    Ok(gas_oracle.propose_gas_price)
}

/// Calculate the maximum refund amount per relay transaction in `wrappedToken`, based on
/// `MAX_REFUND_USD`.
pub async fn max_refund(wrapped_token: &str) -> U256 {
    let prices = COIN_GECKO_CLIENT
        .price(&[wrapped_token], &["usd"], false, false, false, false)
        .await
        .unwrap();
    let wrapped_price = prices[wrapped_token].usd.unwrap();
    let max_refund_wrapped = MAX_REFUND_USD / wrapped_price;

    to_u256(max_refund_wrapped)
}

/// To match types of `ExtData.refund` and `ExtData.fee`, methods here need to return U256,
/// meaning a conversion is necessary. This conversion is done analogous to
/// `ethers::utils::parse_ether`, with the actual amount of digits of USDC.
///
/// TODO: Needs to support other wrapped tokens with different number of digits. It would be easier
///       to simply return the f64 amount, but that would require changing the type param for `ExtData`.
fn to_u256(amount: f64) -> U256 {
    let multiplier = 10_i32.pow(USDC_DECIMALS) as f64;
    let val = amount * multiplier;
    U256::from_dec_str(&val.round().to_string()).unwrap()
}
