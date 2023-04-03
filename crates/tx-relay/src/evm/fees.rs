use crate::{MAX_REFUND_USD, TRANSACTION_PROFIT_USD};
use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use once_cell::sync::Lazy;
use serde::Serialize;
use std::cmp::min;
use std::collections::HashMap;
use std::ops::Add;
use std::sync::{Arc, Mutex};
use webb::evm::contract::protocol_solidity::{
    FungibleTokenWrapperContract, OpenVAnchorContract,
};
use webb::evm::ethers::prelude::U256;
use webb::evm::ethers::providers::Middleware;
use webb::evm::ethers::signers::Signer;
use webb::evm::ethers::types::Address;
use webb::evm::ethers::utils::{format_units, parse_units};
use webb_proposals::TypedChainId;
use webb_relayer_context::RelayerContext;
use webb_relayer_handler_utils::CommandResponse::{Error, Network};
use webb_relayer_handler_utils::{CommandResponse, NetworkStatus};
use webb_relayer_utils::Result;

/// Amount of time for which a `FeeInfo` is valid after creation
static FEE_CACHE_TIME: Lazy<Duration> = Lazy::new(|| Duration::minutes(1));

/// Cache for previously generated fee info. Key consists of the VAnchor address and chain id.
/// Entries are valid as long as `timestamp` is no older than `FEE_CACHE_TIME`.
static FEE_INFO_CACHED: Lazy<
    Mutex<HashMap<(Address, TypedChainId), EvmFeeInfo>>,
> = Lazy::new(|| Mutex::new(HashMap::new()));

/// Return value of fee_info API call. Contains information about relay transaction fee and refunds.
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EvmFeeInfo {
    /// Estimated fee for an average relay transaction, in `wrappedToken`. This is only for
    /// display to the user
    pub estimated_fee: U256,
    /// Price per gas using "normal" confirmation speed, in `nativeToken`
    pub gas_price: U256,
    /// Exchange rate for refund from `wrappedToken` to `nativeToken`
    pub refund_exchange_rate: U256,
    /// Maximum amount of `nativeToken` which can be exchanged to `wrappedToken` by relay
    pub max_refund: U256,
    /// Time when this FeeInfo was generated
    timestamp: DateTime<Utc>,
    /// Price of the native token in USD, internally cached to recalculate estimated fee
    #[serde(skip)]
    native_token_price: f64,
    /// Price of the wrapped token in USD, internally cached to recalculate estimated fee
    #[serde(skip)]
    wrapped_token_price: f64,
    /// Number of decimals of the wrapped token, internally cached to recalculate estimated fee
    #[serde(skip)]
    wrapped_token_decimals: u32,
}

/// Get the current fee info.
///
/// If fee info was recently requested, the cached value is used. Otherwise it is regenerated
/// based on the current exchange rate and estimated gas price.
pub async fn get_evm_fee_info(
    chain_id: TypedChainId,
    vanchor: Address,
    gas_amount: U256,
    ctx: &RelayerContext,
) -> Result<EvmFeeInfo> {
    // Retrieve cached fee info item
    let fee_info_cached = {
        let mut lock = FEE_INFO_CACHED.lock().unwrap();
        // Remove all items from cache which are older than `FEE_CACHE_TIME`
        lock.retain(|_, v| {
            let fee_info_valid_time = v.timestamp.add(*FEE_CACHE_TIME);
            fee_info_valid_time > Utc::now()
        });
        lock.get(&(vanchor, chain_id)).cloned()
    };

    match fee_info_cached {
        // There is a cached fee info, use it
        Some(mut fee_info) => {
            // Need to recalculate estimated fee with the gas amount that was passed in. We use
            // cached exchange rate so that this matches calculation on the client.
            fee_info.estimated_fee = calculate_transaction_fee(
                fee_info.gas_price,
                gas_amount,
                fee_info.native_token_price,
                fee_info.wrapped_token_price,
                fee_info.wrapped_token_decimals,
            )
            .await?;
            Ok(fee_info)
        }
        // No cached fee info, generate new one
        None => {
            let fee_info =
                generate_fee_info(chain_id, vanchor, gas_amount, ctx).await?;

            // Insert newly generated fee info into cache.
            FEE_INFO_CACHED
                .lock()
                .unwrap()
                .insert((vanchor, chain_id), fee_info.clone());
            Ok(fee_info)
        }
    }
}

/// Generate new fee info by fetching relevant data from remote APIs and doing calculations.
async fn generate_fee_info(
    chain_id: TypedChainId,
    vanchor: Address,
    gas_amount: U256,
    ctx: &RelayerContext,
) -> Result<EvmFeeInfo> {
    // Get token names
    let (native_token, native_token_decimals) =
        get_native_token_name_and_decimals(chain_id)?;
    let (wrapped_token, wrapped_token_decimals) =
        get_wrapped_token_name_and_decimals(chain_id, vanchor, ctx).await?;

    // Fetch USD prices for tokens from coingecko API (eg value of 1 ETH in USD).
    let prices = ctx
        .coin_gecko_client()
        .price(
            &[native_token, &wrapped_token],
            &["usd"],
            false,
            false,
            false,
            false,
        )
        .await?;
    let native_token_price = match prices.get(native_token) {
        Some(price) => price.usd.expect("price.usd is not None"),
        None => {
            return Err(webb_relayer_utils::Error::FetchTokenPriceError {
                token: native_token.into(),
            })
        }
    };
    // try to get wrapped token price from coingecko, if not found, use the price from config
    // if not found in config, return error
    let maybe_wrapped_token_price = prices
        .get(&wrapped_token)
        .and_then(|p| p.usd)
        .or(ctx.config.assets.get(&wrapped_token).map(|a| a.price));

    let wrapped_token_price = match maybe_wrapped_token_price {
        Some(price) => price,
        None => {
            return Err(webb_relayer_utils::Error::FetchTokenPriceError {
                token: wrapped_token.clone(),
            })
        }
    };
    // Fetch native gas price estimate from etherscan.io, using "average" value
    let gas_oracle = ctx
        .etherscan_client(chain_id.underlying_chain_id())?
        .gas_oracle()
        .await?;
    let gas_price_gwei = U256::from(gas_oracle.propose_gas_price);
    let gas_price = parse_units(gas_price_gwei, "gwei")?.into();

    let estimated_fee = calculate_transaction_fee(
        gas_price,
        gas_amount,
        native_token_price,
        wrapped_token_price,
        wrapped_token_decimals,
    )
    .await?;

    // Calculate the exchange rate from wrapped token to native token which is used for the refund.
    let refund_exchange_rate = parse_units(
        native_token_price / wrapped_token_price,
        wrapped_token_decimals,
    )?
    .into();

    // Calculate the maximum refund amount per relay transaction in `nativeToken`.
    // Ensuring that refund <= relayer balance
    // TODO: should also check balance when returning cached value
    let relayer_balance =
        relayer_balance(chain_id.underlying_chain_id(), ctx).await.unwrap();
    let max_refund =
        parse_units(MAX_REFUND_USD / native_token_price, native_token_decimals)?
            .into();
    let max_refund = min(relayer_balance, max_refund);

    Ok(EvmFeeInfo {
        estimated_fee,
        gas_price,
        refund_exchange_rate,
        max_refund,
        timestamp: Utc::now(),
        native_token_price,
        wrapped_token_price,
        wrapped_token_decimals,
    })
}

async fn relayer_balance(
    chain_id: u32,
    ctx: &RelayerContext,
) -> std::result::Result<U256, CommandResponse> {
    let wallet = ctx.evm_wallet(&chain_id.to_string()).await.map_err(|e| {
        Error(format!("Misconfigured Network: {:?}, {e}", chain_id))
    })?;
    let provider =
        ctx.evm_provider(&chain_id.to_string()).await.map_err(|e| {
            Network(NetworkStatus::Failed {
                reason: e.to_string(),
            })
        })?;
    let relayer_balance = provider
        .get_balance(wallet.address(), None)
        .await
        .map_err(|e| {
            Error(format!("Failed to retrieve relayer balance: {e}"))
        })?;
    Ok(relayer_balance)
}

/// Pull USD prices of base token from coingecko.com, and use this to calculate the transaction
/// fee in `wrappedToken` wei. This fee includes a profit for the relay of `TRANSACTION_PROFIT_USD`.
///
/// The algorithm is explained at https://www.notion.so/hicommonwealth/Private-Tx-Relay-Support-v1-f5522b04d6a349aab1bbdb0dd83a7fb4#6bb2b4920e3f42d69988688c6fa54e6e
async fn calculate_transaction_fee(
    gas_price: U256,
    gas_amount: U256,
    native_token_price: f64,
    wrapped_token_price: f64,
    wrapped_token_decimals: u32,
) -> Result<U256> {
    // Step 1: Calculate the tx fee in native token (in wei)
    let tx_fee_native_token_wei = gas_price * gas_amount;
    let tx_fee_native_token = format_units(tx_fee_native_token_wei, "ether")?;
    // Step 2: Convert the tx fee to USD using the coingecko API.
    let tx_fee_tokens = tx_fee_native_token
        .parse::<f64>()
        .expect("Failed to parse tx fee");
    let tx_fee_usd = tx_fee_tokens * native_token_price;
    // Step 3: Calculate the profit that the relayer should make, and add it to the tx fee in USD.
    // This is the total amount of USD that the relayer should receive.
    let total_fee_with_profit_in_usd = tx_fee_usd + TRANSACTION_PROFIT_USD;
    // Step 4: Convert the total fee to `wrappedToken` using the exchange rate for the underlying
    // wrapped token.
    // This is the total amount of `wrappedToken` that the relayer should receive.
    // This is in `wrappedToken` units, not wei.
    let total_fee_tokens = total_fee_with_profit_in_usd / wrapped_token_price;
    // Step 5: Convert the result to wei and return it.
    let fee_with_profit =
        parse_units(total_fee_tokens, wrapped_token_decimals)?.into();
    Ok(fee_with_profit)
}

/// Retrieves the token name of a given anchor contract. Wrapper prefixes are stripped in order
/// to get a token name which coingecko understands.
async fn get_wrapped_token_name_and_decimals(
    chain_id: TypedChainId,
    vanchor: Address,
    ctx: &RelayerContext,
) -> Result<(String, u32)> {
    let chain_name = chain_id.underlying_chain_id().to_string();
    let provider = ctx.evm_provider(&chain_name).await?;
    let client = Arc::new(provider);

    let anchor_contract = OpenVAnchorContract::new(vanchor, client.clone());
    let token_address = anchor_contract.token().call().await?;
    let token_contract =
        FungibleTokenWrapperContract::new(token_address, client.clone());
    let token_symbol = token_contract.symbol().call().await?;
    // TODO: add all supported tokens
    let name = match token_symbol.replace("webb", "").as_str() {
        "WETH" => "ethereum",
        // only used in tests
        "WEBB" if cfg!(debug_assertions) => "ethereum",
        x => x,
    }
    .to_string();
    let decimals = token_contract.decimals().call().await?;
    Ok((name, decimals.into()))
}

/// Hardcodede mapping from chain id to base token name. Testnets use the mainnet name because
/// otherwise there is no exchange rate available.
///
/// https://github.com/DefiLlama/chainlist/blob/main/constants/chainIds.json
fn get_native_token_name_and_decimals(
    chain_id: TypedChainId,
) -> Result<(&'static str, i32)> {
    use TypedChainId::*;
    let name = match chain_id {
        Evm(id) => {
            match id {
                1 | // ethereum mainnet
                    5 | // goerli testnet
                    5001 | // hermes testnet
                    5002 | // athena testnet
                    5003 | // demeter testnet
                    11155111 // sepolia testnet
                => "ethereum",
                // optimism mainnet and testnet
                10 | 420 => "optimism",
                // polygon mainnet and testnet
                127 | 80001 => "polygon",
                // moonbeam mainnet and testnet
                1284 | 1287 => "moonbeam",
                _ => {
                    // Typescript tests use randomly generated chain id, so we always return
                    // "ethereum" in debug mode to make them work.
                    if cfg!(debug_assertions) {
                        "ethereum"
                    } else {
                        let chain_id = chain_id.chain_id().to_string();
                        return Err(webb_relayer_utils::Error::ChainNotFound { chain_id });
                    }
                }
            }
        }
        _ => {
            return Err(webb_relayer_utils::Error::ChainNotFound {
                chain_id: chain_id.chain_id().to_string(),
            })
        }
    };
    let decimals = 18;
    Ok((name, decimals))
}
