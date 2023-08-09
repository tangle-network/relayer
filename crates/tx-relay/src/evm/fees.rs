use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use once_cell::sync::Lazy;
use serde::Serialize;
use std::cmp::min;
use std::collections::HashMap;
use std::ops::Add;
use std::sync::{Arc, Mutex};
use webb::evm::contract::protocol_solidity::fungible_token_wrapper::FungibleTokenWrapperContract;
use webb::evm::contract::protocol_solidity::variable_anchor::VAnchorContract;
use webb::evm::ethers::middleware::gas_oracle::GasOracle;
use webb::evm::ethers::prelude::U256;
use webb::evm::ethers::providers::Middleware;
use webb::evm::ethers::signers::Signer;
use webb::evm::ethers::types::Address;
use webb::evm::ethers::utils::{format_units, parse_units};
use webb_chains_info::chain_info_by_chain_id;
use webb_price_oracle_backends::PriceBackend;
use webb_proposals::TypedChainId;
use webb_relayer_config::evm::RelayerFeeConfig;
use webb_relayer_context::RelayerContext;
use webb_relayer_utils::Result;

/// Amount of time for which a `FeeInfo` is valid after creation
const FEE_CACHE_TIME: core::time::Duration =
    core::time::Duration::from_secs(60);

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
    /// Number of decimals of the native token, internally cached to recalculate max refund
    #[serde(skip)]
    native_token_decimals: u8,
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
    let requested_chain = chain_id.underlying_chain_id();
    let chain_config = ctx.config.evm.get(&requested_chain.to_string()).ok_or(
        webb_relayer_utils::Error::ChainNotFound {
            chain_id: requested_chain.to_string(),
        },
    )?;

    // Retrieve cached fee info item
    let fee_info_cached = {
        let mut lock =
            FEE_INFO_CACHED.lock().expect("lock fee info cache mutex");
        // Remove all items from cache which are older than `FEE_CACHE_TIME`
        lock.retain(|_, v| {
            let fee_info_valid_time =
                v.timestamp.add(Duration::from_std(FEE_CACHE_TIME).expect(
                    "FEE_CACHE_TIME must be convertible to chrono::Duration",
                ));
            fee_info_valid_time > Utc::now()
        });
        lock.get(&(vanchor, chain_id)).cloned()
    };

    if let Some(mut fee_info) = fee_info_cached {
        // Need to recalculate estimated fee with the gas amount that was passed in. We use
        // cached exchange rate so that this matches calculation on the client.
        fee_info.estimated_fee = calculate_transaction_fee(
            &chain_config.relayer_fee_config,
            fee_info.gas_price,
            gas_amount,
            fee_info.native_token_price,
            fee_info.wrapped_token_price,
            fee_info.wrapped_token_decimals,
        )?;
        // Recalculate max refund in case relayer balance changed.
        fee_info.max_refund = max_refund(
            chain_id,
            &chain_config.relayer_fee_config,
            fee_info.native_token_price,
            fee_info.native_token_decimals,
            ctx,
        )
        .await?;
        Ok(fee_info)
    } else {
        let fee_info = generate_fee_info(
            chain_id,
            &chain_config.relayer_fee_config,
            vanchor,
            gas_amount,
            ctx,
        )
        .await?;

        // Insert newly generated fee info into cache.
        FEE_INFO_CACHED
            .lock()
            .expect("lock fee info cache mutex")
            .insert((vanchor, chain_id), fee_info.clone());
        Ok(fee_info)
    }
}

/// Generate new fee info by fetching relevant data from remote APIs and doing calculations.
async fn generate_fee_info(
    chain_id: TypedChainId,
    relayer_fee_config: &RelayerFeeConfig,
    vanchor: Address,
    gas_amount: U256,
    ctx: &RelayerContext,
) -> Result<EvmFeeInfo> {
    // Get token names
    let (native_token, native_token_decimals) =
        get_native_token_name_and_decimals(chain_id)?;
    let (wrapped_token, wrapped_token_decimals) =
        get_wrapped_token_name_and_decimals(chain_id, vanchor, ctx).await?;

    // Fetch USD prices for tokens from the price oracle backend (eg value of 1 ETH in USD).
    let prices = ctx
        .price_oracle()
        .get_prices(&[native_token, &wrapped_token])
        .await?;

    let native_token_price = match prices.get(native_token) {
        Some(price) => *price,
        None => {
            return Err(webb_relayer_utils::Error::FetchTokenPriceError {
                token: native_token.into(),
            })
        }
    };

    let wrapped_token_price = match prices.get(&wrapped_token) {
        Some(price) => *price,
        None => {
            return Err(webb_relayer_utils::Error::FetchTokenPriceError {
                token: wrapped_token.clone(),
            })
        }
    };

    // Fetch native gas price estimate from gas oracle, using "average" value
    let gas_price = ctx
        .gas_oracle(chain_id.underlying_chain_id())
        .await?
        .fetch()
        .await?;

    let estimated_fee = calculate_transaction_fee(
        relayer_fee_config,
        gas_price,
        gas_amount,
        native_token_price,
        wrapped_token_price,
        wrapped_token_decimals,
    )?;

    // Calculate the exchange rate from wrapped token to native token which is used for the refund.
    let refund_exchange_rate = parse_units(
        native_token_price / wrapped_token_price,
        wrapped_token_decimals,
    )?
    .into();

    Ok(EvmFeeInfo {
        estimated_fee,
        gas_price,
        refund_exchange_rate,
        max_refund: max_refund(
            chain_id,
            relayer_fee_config,
            native_token_price,
            native_token_decimals,
            ctx,
        )
        .await?,
        timestamp: Utc::now(),
        native_token_price,
        native_token_decimals,
        wrapped_token_price,
        wrapped_token_decimals,
    })
}

async fn max_refund(
    chain_id: TypedChainId,
    relayer_fee_config: &RelayerFeeConfig,
    native_token_price: f64,
    native_token_decimals: u8,
    ctx: &RelayerContext,
) -> Result<U256> {
    let wallet = ctx.evm_wallet(chain_id.underlying_chain_id()).await?;
    let provider = ctx.evm_provider(chain_id.underlying_chain_id()).await?;
    let relayer_balance = provider.get_balance(wallet.address(), None).await?;

    // Get the maximum refund amount in USD from the config.
    let max_refund_amount = relayer_fee_config.max_refund_amount;

    // Calculate the maximum refund amount per relay transaction in `nativeToken`.
    // Ensuring that refund <= relayer balance
    let max_refund = parse_units(
        max_refund_amount / native_token_price,
        u32::from(native_token_decimals),
    )?
    .into();
    Ok(min(relayer_balance, max_refund))
}

/// Pull USD prices of base token from coingecko.com, and use this to calculate the transaction
/// fee in `wrappedToken` wei. This fee includes a profit for the relay of `TRANSACTION_PROFIT_USD`.
///
/// The algorithm is explained at https://www.notion.so/hicommonwealth/Private-Tx-Relay-Support-v1-f5522b04d6a349aab1bbdb0dd83a7fb4#6bb2b4920e3f42d69988688c6fa54e6e
fn calculate_transaction_fee(
    relayer_fee_config: &RelayerFeeConfig,
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
    let relay_tx_profit =
        (relayer_fee_config.relayer_profit_percent / 100.0) * tx_fee_usd;
    let total_fee_with_profit_in_usd = tx_fee_usd + relay_tx_profit;
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

/// Returns the name and decimals of the wrapped token for the given chain.
/// then converts it to the underlying token token name to be used in the price oracle.
async fn get_wrapped_token_name_and_decimals(
    chain_id: TypedChainId,
    vanchor: Address,
    ctx: &RelayerContext,
) -> Result<(String, u32)> {
    let provider = ctx.evm_provider(chain_id.underlying_chain_id()).await?;
    let client = Arc::new(provider);

    let anchor_contract = VAnchorContract::new(vanchor, client.clone());
    let token_address = anchor_contract.token().call().await?;
    let token_contract =
        FungibleTokenWrapperContract::new(token_address, client.clone());
    let token_symbol = token_contract.symbol().call().await?;
    // TODO: add all supported tokens
    let name = match token_symbol.replace("webb", "").as_str() {
        "Alpha" | "Standalone" | "WETH" => "ETH",
        "tTNT-standalone" => "tTNT",
        // only used in tests
        "WEBB" if cfg!(debug_assertions) => "ETH",
        x => x,
    }
    .to_string();
    let decimals = token_contract.decimals().call().await?;
    Ok((name, decimals.into()))
}

/// Returns the native token symbol and the decimals
/// of the given chain identifier
fn get_native_token_name_and_decimals(
    chain_id: TypedChainId,
) -> Result<(&'static str, u8)> {
    use TypedChainId::*;
    match chain_id {
        Evm(id) => chain_info_by_chain_id(u64::from(id)).map_or_else(
            || {
                // Typescript tests use randomly generated chain id, so we always return
                // "ethereum" in debug mode to make them work.
                if cfg!(debug_assertions) {
                    Ok(("ETH", 18))
                } else {
                    let chain_id = chain_id.chain_id().to_string();
                    Err(webb_relayer_utils::Error::ChainNotFound { chain_id })
                }
            },
            |info| {
                Ok((info.native_currency.symbol, info.native_currency.decimals))
            },
        ),
        Substrate(id) => match id {
            1081 => Ok(("tTNT", 18)),
            _ => {
                // During testing, we will use the tTNT token for all substrate chains.
                if cfg!(debug_assertions) {
                    Ok(("tTNT", 18))
                } else {
                    let chain_id = chain_id.chain_id().to_string();
                    Err(webb_relayer_utils::Error::ChainNotFound { chain_id })
                }
            }
        },
        unknown => Err(webb_relayer_utils::Error::ChainNotFound {
            chain_id: unknown.chain_id().to_string(),
        }),
    }
}
