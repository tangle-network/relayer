use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use coingecko::CoinGeckoClient;
use serde::Serialize;
use std::net::IpAddr;
use std::sync::Arc;
use std::sync::Mutex;
use webb::evm::ethers;
use webb::evm::ethers::types::Chain;
use webb_relayer_context::RelayerContext;

static FEE_DATA: Mutex<Vec<FeeInfo>> = Mutex::new(Vec::new());

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FeeInfo {
    timestamp: DateTime<Utc>,
    client_ip: IpAddr,
    fee: f32,
    refund_exchange_rate: f32,
    max_refund: f32,
}

pub async fn calculate_fees(
    chain_id: u32,
    client_ip: IpAddr,
    ctx: Arc<RelayerContext>,
) -> webb_relayer_utils::Result<FeeInfo> {
    let chain = ctx.config.evm.get(&chain_id.to_string()).unwrap();

    // TODO: these are all hardcoded for now
    let gas_amount = 1_721_713;
    let max_refund = 10.;

    let gas_price = estimate_gas_price().await.unwrap();
    dbg!(gas_price);

    let exchange_rate =
        calculate_exchange_rate("usd-coin", "ethereum").await as f32;
    dbg!(exchange_rate);

    // TODO: we might want to multiply this by a constant factor, in case the actual gas price
    //       goes up by the time the transaction is submitted
    let native_fee = (gas_price * gas_amount) as f32;
    let wrapped_fee = native_fee * exchange_rate;
    clear_outdated_fee_data();
    Ok(FeeInfo {
        timestamp: Utc::now(),
        client_ip,
        fee: wrapped_fee,
        refund_exchange_rate: exchange_rate,
        max_refund,
    })
}

/// Relay fees are only valid for a limited amount of time. When that time is expired, remove
/// the entry so that it can't be used anymore.
fn clear_outdated_fee_data() {
    let mut lock = FEE_DATA.lock().unwrap();
    let expiration_time = Duration::minutes(1);
    let updated = lock
        .iter()
        .filter(|f| f.timestamp + expiration_time >= Utc::now())
        .cloned()
        .collect::<Vec<FeeInfo>>();
    *lock = updated;
}

/// Estimate gas price using etherscan.io. Note that this functionality is only available
/// on mainnet.
async fn estimate_gas_price() -> webb_relayer_utils::Result<u64> {
    // fee estimation using etherscan, only supports mainnet
    let client = ethers::etherscan::Client::builder()
        // TODO: need to add actual api key via config
        .with_api_key("YourApiKeyToken")
        .chain(Chain::Mainnet)
        .unwrap()
        .build()
        .unwrap();
    // using "average" gas price
    Ok(client.gas_oracle().await.unwrap().propose_gas_price)
}

/// Pull USD prices of wrapped token and base token from coingecko.com, and use these to
/// calculate the exchange rate.
async fn calculate_exchange_rate(wrapped_token: &str, base_token: &str) -> f64 {
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
