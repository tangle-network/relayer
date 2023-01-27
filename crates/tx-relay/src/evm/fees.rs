use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use ethers::middleware::SignerMiddleware;
use ethers::types::Address;
use ethers::utils::{format_units, parse_ether, parse_units};
use once_cell::sync::Lazy;
use serde::Serialize;
use std::collections::HashMap;
use std::ops::Add;
use std::sync::{Arc, Mutex};
use webb::evm::contract::protocol_solidity::{
    FungibleTokenWrapperContract, OpenVAnchorContract,
};
use webb::evm::ethers::prelude::U256;
use webb_relayer_context::RelayerContext;
use webb_relayer_utils::Result;

/// Maximum refund amount per relay transaction in USD.
const MAX_REFUND_USD: f64 = 5.;
/// Amount of time for which a `FeeInfo` is valid after creation
static FEE_CACHE_TIME: Lazy<Duration> = Lazy::new(|| Duration::minutes(1));
/// Amount of profit that the relay should make with each transaction (in USD).
const TRANSACTION_PROFIT_USD: f64 = 5.;

/// Cache for previously generated fee info. Key consists of the VAnchor address and chain id.
/// Entries are valid as long as `timestamp` is no older than `FEE_CACHE_TIME`.
static FEE_INFO_CACHED: Lazy<Mutex<HashMap<(Address, u64), FeeInfo>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Return value of fee_info API call. Contains information about relay transaction fee and refunds.
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FeeInfo {
    /// Estimated fee for an average relay transaction, in `wrappedToken`. This is only for
    /// display to the user
    pub estimated_fee: U256,
    /// Price per gas using "normal" confirmation speed, in `nativeToken`
    pub gas_price: U256,
    /// Exchange rate for refund from `wrappedToken` to `nativeToken`
    pub refund_exchange_rate: f64,
    /// Maximum amount of `wrappedToken` which can be exchanged to `nativeToken` by relay
    pub max_refund: U256,
    /// Time when this FeeInfo was generated
    timestamp: DateTime<Utc>,
}

/// Get the current fee info.
///
/// If fee info was recently requested, the cached value is used. Otherwise it is regenerated
/// based on the current exchange rate and estimated gas price.
pub async fn get_fee_info(
    chain_id: u64,
    vanchor: Address,
    estimated_gas_amount: U256,
    ctx: &RelayerContext,
) -> Result<FeeInfo> {
    evict_cache();
    // Check if there is an existing fee info. Return it directly if thats the case.
    {
        let lock = FEE_INFO_CACHED.lock().unwrap();
        if let Some(fee_info) = lock.get(&(vanchor, chain_id)) {
            return Ok(fee_info.clone());
        }
    }

    let gas_price = estimate_gas_price(ctx).await?;
    let estimated_fee = calculate_transaction_fee(
        gas_price,
        estimated_gas_amount,
        chain_id,
        vanchor,
        ctx,
    )
    .await?;
    let refund_exchange_rate =
        calculate_refund_exchange_rate(chain_id, vanchor, ctx).await?;
    let max_refund = max_refund(chain_id, vanchor, ctx).await?;

    let fee_info = FeeInfo {
        estimated_fee,
        gas_price,
        refund_exchange_rate,
        max_refund,
        timestamp: Utc::now(),
    };
    // Insert newly generated fee info into cache.
    FEE_INFO_CACHED
        .lock()
        .unwrap()
        .insert((vanchor, chain_id), fee_info.clone());
    Ok(fee_info)
}

/// Remove all items from fee_info cache which are older than `FEE_CACHE_TIME`.
fn evict_cache() {
    let mut cache = FEE_INFO_CACHED.lock().unwrap();
    cache.retain(|_, v| {
        let fee_info_valid_time = v.timestamp.add(*FEE_CACHE_TIME);
        fee_info_valid_time > Utc::now()
    });
}

/// Pull USD prices of base token from coingecko.com, and use this to calculate the transaction
/// fee in `wrappedToken` wei. This fee includes a profit for the relay of `TRANSACTION_PROFIT_USD`.
///
/// The algorithm is explained at https://www.notion.so/hicommonwealth/Private-Tx-Relay-Support-v1-f5522b04d6a349aab1bbdb0dd83a7fb4#6bb2b4920e3f42d69988688c6fa54e6e
async fn calculate_transaction_fee(
    gas_price: U256,
    gas_amount: U256,
    chain_id: u64,
    vanchor: Address,
    ctx: &RelayerContext,
) -> Result<U256> {
    // Step 1: Calculate the tx fee in native token (in wei)
    let tx_fee_native_token_wei = gas_price * gas_amount;
    let tx_fee_native_token = format_units(tx_fee_native_token_wei, "ether")?;
    // Step 2: Convert the tx fee to USD using the coingecko API.
    let native_token = get_base_token_name(chain_id)?;
    // This the price of 1 native token in USD (e.g. 1 ETH in USD)
    let native_token_price = fetch_token_price(&native_token, ctx).await?;
    let tx_fee_tokens = tx_fee_native_token
        .parse::<f64>()
        .expect("Failed to parse tx fee");
    let tx_fee_usd = tx_fee_tokens * native_token_price;
    // Step 3: Calculate the profit that the relayer should make, and add it to the tx fee in USD.
    // This is the total amount of USD that the relayer should receive.
    let total_fee_with_profit_in_usd = tx_fee_usd + TRANSACTION_PROFIT_USD;
    // Step 4: Convert the total fee to `wrappedToken` using the exchange rate for the underlying
    // wrapped token.
    let wrapped_token = get_wrapped_token_name(chain_id, vanchor, ctx).await?;
    let wrapped_token_price = fetch_token_price(wrapped_token, ctx).await?;
    // This is the total amount of `wrappedToken` that the relayer should receive.
    // This is in `wrappedToken` units, not wei.
    let total_fee_tokens = total_fee_with_profit_in_usd / wrapped_token_price;
    // Step 5: Convert the result to wei and return it.
    // TODO: Hardcoded decimals for wrapped token. This should be fetched from the contract.
    let fee_with_profit = parse_units(total_fee_tokens, 18)?;
    Ok(fee_with_profit)
}

/// Calculate the exchange rate from wrapped token to native token which is used for the refund.
async fn calculate_refund_exchange_rate(
    chain_id: u64,
    vanchor: Address,
    ctx: &RelayerContext,
) -> Result<f64> {
    let base_token = get_base_token_name(chain_id)?;
    let base_token_price = fetch_token_price(base_token, ctx).await?;
    let wrapped_token = get_wrapped_token_name(chain_id, vanchor, ctx).await?;
    let wrapped_token_price = fetch_token_price(wrapped_token, ctx).await?;

    let exchange_rate = base_token_price / wrapped_token_price;
    Ok(exchange_rate)
}

/// Estimate gas price using etherscan.io. Note that this functionality is only available
/// on mainnet.
async fn estimate_gas_price(ctx: &RelayerContext) -> Result<U256> {
    let gas_oracle = ctx.etherscan_client().gas_oracle().await?;
    // use the "average" gas price
    let gas_price_gwei = U256::from(gas_oracle.propose_gas_price);
    Ok(parse_units(gas_price_gwei, "gwei")?)
}

/// Calculate the maximum refund amount per relay transaction in `wrappedToken`, based on
/// `MAX_REFUND_USD`.
async fn max_refund(
    chain_id: u64,
    vanchor: Address,
    ctx: &RelayerContext,
) -> Result<U256> {
    let wrapped_token = get_wrapped_token_name(chain_id, vanchor, ctx).await?;
    let wrapped_token_price = fetch_token_price(wrapped_token, ctx).await?;
    let max_refund_wrapped = MAX_REFUND_USD / wrapped_token_price;

    Ok(parse_ether(max_refund_wrapped)?)
}

/// Fetch USD price for the given token from coingecko API.
async fn fetch_token_price<Id: AsRef<str>>(
    token_name: Id,
    ctx: &RelayerContext,
) -> Result<f64> {
    let prices = ctx
        .coin_gecko_client()
        .price(&[token_name.as_ref()], &["usd"], false, false, false, false)
        .await?;
    Ok(prices[token_name.as_ref()].usd.unwrap())
}

/// Retrieves the token name of a given anchor contract. Wrapper prefixes are stripped in order
/// to get a token name which coingecko understands.
async fn get_wrapped_token_name(
    chain_id: u64,
    vanchor: Address,
    ctx: &RelayerContext,
) -> Result<String> {
    let chain_name = chain_id.to_string();
    let wallet = ctx.evm_wallet(&chain_name).await?;
    let provider = ctx.evm_provider(&chain_name).await?;
    let client = Arc::new(SignerMiddleware::new(provider, wallet));

    let anchor_contract = OpenVAnchorContract::new(vanchor, client.clone());
    let token_address = anchor_contract.token().call().await?;
    let token_contract =
        FungibleTokenWrapperContract::new(token_address, client.clone());
    let token_symbol = token_contract.symbol().call().await?;
    // TODO: add all supported tokens
    Ok(match token_symbol.replace("webb", "").as_str() {
        "WETH" => "ethereum",
        // only used in tests
        "WEBB" if cfg!(debug_assertions) => "ethereum",
        x => x,
    }
    .to_string())
}

/// Hardcodede mapping from chain id to base token name. Testnets use the mainnet name because
/// otherwise there is no exchange rate available.
///
/// https://github.com/DefiLlama/chainlist/blob/main/constants/chainIds.json
fn get_base_token_name(chain_id: u64) -> Result<&'static str> {
    match chain_id {
        1 | 5 | 5001 | 5002 | 5003 | 11155111 => Ok("ethereum"),
        10 | 420 => Ok("optimism"),
        127 | 80001 => Ok("polygon"),
        1284 | 1287 => Ok("moonbeam"),
        _ => {
            // Typescript tests use randomly generated chain id, so we always return "ethereum"
            // in debug mode to make them work.
            if cfg!(debug_assertions) {
                Ok("ethereum")
            } else {
                let chain_id = chain_id.to_string();
                Err(webb_relayer_utils::Error::ChainNotFound { chain_id })
            }
        }
    }
}
