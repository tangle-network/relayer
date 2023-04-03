use std::cmp::min;
use crate::{MAX_REFUND_USD, TRANSACTION_PROFIT_USD};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde::{Serialize};
use sp_core::U256;
use webb::substrate::subxt::{PolkadotConfig, SubstrateConfig};
use webb::substrate::protocol_substrate_runtime::api as RuntimeApi;
use webb::substrate::subxt::tx::PairSigner;
use webb_relayer_context::RelayerContext;

const TOKEN_PRICE_USD: f64 = 0.1;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SubstrateFeeInfo {
    /// Estimated fee for an average relay transaction, in `wrappedToken`. Includes network fees
    /// and relay fee.
    pub estimated_fee: U256,
    /// Exchange rate for refund from `wrappedToken` to `nativeToken`
    pub refund_exchange_rate: U256,
    /// Maximum amount of `nativeToken` which can be exchanged to `wrappedToken` by relay
    pub max_refund: U256,
    /// Time when this FeeInfo was generated
    timestamp: DateTime<Utc>,
}

pub async fn get_substrate_fee_info(
    chain_id: u64,
    estimated_tx_fees: U256,
    ctx: &RelayerContext,
) -> webb_relayer_utils::Result<SubstrateFeeInfo> {
    let client = ctx
        .substrate_provider::<PolkadotConfig>(&chain_id.to_string())
        .await
        .unwrap();
    let decimals: i32 = client
        .rpc()
        .system_properties()
        .await
        .unwrap()
        .get("tokenDecimals")
        .unwrap()
        .as_i64()
        .unwrap() as i32;
    let estimated_fee = dbg!(U256::from(
        dbg!(estimated_tx_fees)
            + dbg!(native_token_to_unit(
                TRANSACTION_PROFIT_USD / TOKEN_PRICE_USD,
                decimals,
            )),
    ));
    let refund_exchange_rate = native_token_to_unit(1., decimals);
    // TODO: should ensure that refund <= relayer balance
    let max_refund =
        native_token_to_unit(MAX_REFUND_USD / TOKEN_PRICE_USD, decimals);
    let max_refund = min(max_refund, relayer_balance(chain_id, ctx).await?);
    Ok(SubstrateFeeInfo {
        estimated_fee,
        refund_exchange_rate,
        max_refund,
        timestamp: Utc::now(),
    })
}

async fn relayer_balance(chain_id: u64,
    ctx: &RelayerContext,) -> webb_relayer_utils::Result<U256>{
    let client = ctx
        .substrate_provider::<SubstrateConfig>(&chain_id.to_string())
        .await?;
    let pair = ctx
        .substrate_wallet(&chain_id.to_string())
        .await?;

    let signer = PairSigner::<SubstrateConfig, sp_core::sr25519::Pair>::new(pair.clone());

    let account = RuntimeApi::storage()
        .system()
        .account(signer.account_id());

    let balance = client
        .storage()
        .at(None)
        .await
        .unwrap()
        .fetch(&account)
        .await
        .unwrap();
    Ok(U256::from(balance.unwrap().data.free))
}

/// Convert from full wrapped token amount to smallest unit amount.
///
/// It looks like subxt has no built-in functionality for this.
fn native_token_to_unit(matic: f64, token_decimals: i32) -> U256 {
    U256::from((matic * 10_f64.powi(token_decimals)) as u128)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct RpcFeeDetailsResponse {
    pub partial_fee: U256,
}
