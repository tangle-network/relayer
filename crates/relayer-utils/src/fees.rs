use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use coingecko::CoinGeckoClient;
use ethers::etherscan;
use ethers::types::Chain;
use once_cell::sync::Lazy;
use serde::Serialize;
use std::ops::Add;
use std::sync::Mutex;
use webb::evm::ethers::prelude::U256;

/// Maximum refund amount per relay transaction in USD.
const MAX_REFUND_USD: f64 = 1.;
/// Amount of time for which a `FeeInfo` is valid after creation
const FEE_CACHE_TIME: Lazy<Duration> = Lazy::new(|| Duration::minutes(1));
/// Number of digits after the comma of USD-Coin.
const USDC_DECIMALS: u32 = 6;

static COIN_GECKO_CLIENT: Lazy<CoinGeckoClient> =
    Lazy::new(CoinGeckoClient::default);
static ETHERSCAN_CLIENT: Lazy<etherscan::Client> =
    Lazy::new(|| etherscan::Client::new_from_env(Chain::Mainnet).unwrap());
/// Fee info which was previously generated. It is still valid if `timestamp` is no older than
/// `FEE_CACHE_TIME`.
static FEE_INFO_CACHED: Mutex<Option<FeeInfo>> = Mutex::new(None);

/// Return value of fee_info API call. Contains information about relay transaction fee and refunds.
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FeeInfo {
    /// Estimated fee for an average relay transaction, in `wrappedToken`. This is only for
    /// display to the user
    estimated_fee: U256,
    /// Price per gas using "normal" confirmation speed, in `nativeToken`
    pub gas_price: U256,
    /// Exchange rate for refund from `wrappedToken` to `nativeToken`
    pub refund_exchange_rate: U256,
    /// Maximum amount of `wrappedToken` which can be exchanged to `nativeToken` by relay
    pub max_refund: U256,
    timestamp: DateTime<Utc>,
}

/// Get the current fee info.
///
/// If fee info was recently requested, the cached value is used. Otherwise it is regenerated
/// based on the current exchange rate and estimated gas price.
pub async fn get_fee_info(
    estimated_gas_amount: U256,
    wrapped_token: &str,
    base_token: &str,
) -> FeeInfo {
    // Check if there is an existing, recent fee info. Return it directly if thats the case.
    {
        let lock = FEE_INFO_CACHED.lock().unwrap();
        if let Some(fee_info) = &*lock {
            let fee_info_valid_time = fee_info.timestamp.add(*FEE_CACHE_TIME);
            if fee_info_valid_time > Utc::now() {
                return fee_info.clone();
            }
        }
    }
    let exchange_rate =
        calculate_exchange_rate(wrapped_token, base_token).await;

    let gas_price = estimate_gas_price().await;
    let wrapped_fee =
        calculate_wrapped_fee(gas_price, estimated_gas_amount, exchange_rate)
            .await;
    let max_refund = max_refund(wrapped_token).await;

    let fee_info = FeeInfo {
        estimated_fee: wrapped_fee,
        gas_price,
        refund_exchange_rate: exchange_rate,
        max_refund,
        timestamp: Utc::now(),
    };
    *FEE_INFO_CACHED.lock().unwrap() = Some(fee_info.clone());
    fee_info
}

/// Calculate fee in `wrappedToken`, using the estimated gas price from etherscan.
pub async fn calculate_wrapped_fee(
    gas_price: U256,
    estimated_gas_amount: U256,
    exchange_rate: U256,
) -> U256 {
    let native_fee = gas_price * estimated_gas_amount;
    native_fee * exchange_rate
}

/// Pull USD prices of wrapped token and base token from coingecko.com, and use these to
/// calculate the exchange rate.
async fn calculate_exchange_rate(
    wrapped_token: &str,
    base_token: &str,
) -> U256 {
    let tokens = &[wrapped_token, base_token];
    let prices = COIN_GECKO_CLIENT
        .price(tokens, &["usd"], false, false, false, false)
        .await
        .unwrap();
    let wrapped_price = prices[wrapped_token].usd.unwrap();
    let base_price = prices[base_token].usd.unwrap();
    let exchange_rate = wrapped_price / base_price;
    to_u256(exchange_rate)
}

/// Estimate gas price using etherscan.io. Note that this functionality is only available
/// on mainnet.
async fn estimate_gas_price() -> U256 {
    let gas_oracle = ETHERSCAN_CLIENT.gas_oracle().await.unwrap();
    // use the "average" gas price
    U256::from(gas_oracle.propose_gas_price)
}

/// Calculate the maximum refund amount per relay transaction in `wrappedToken`, based on
/// `MAX_REFUND_USD`.
async fn max_refund(wrapped_token: &str) -> U256 {
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
