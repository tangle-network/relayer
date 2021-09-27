#![allow(clippy::large_enum_variant)]

use std::collections::HashMap;
use std::convert::Infallible;
use std::error::Error;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use async_stream::stream;
use futures::prelude::*;
use futures::stream::BoxStream;
use serde::{Deserialize, Serialize};
use warp::ws::Message;
use webb::evm::contract::tornado::AnchorContract;
use webb::evm::ethereum_types::{Address, H256, U256};
use webb::evm::ethers::core::k256::SecretKey;
use webb::evm::ethers::prelude::*;
use webb::evm::ethers::types::Bytes;

use crate::context::RelayerContext;
use crate::store::LeafCacheStore;

pub async fn accept_connection(
    ctx: &RelayerContext,
    stream: warp::ws::WebSocket,
) -> anyhow::Result<()> {
    let (mut tx, mut rx) = stream.split();
    while let Some(msg) = rx.try_next().await? {
        if let Ok(text) = msg.to_str() {
            handle_text(ctx, text, &mut tx).await?;
        }
    }
    Ok(())
}

pub async fn handle_text<TX>(
    ctx: &RelayerContext,
    v: &str,
    tx: &mut TX,
) -> anyhow::Result<()>
where
    TX: Sink<Message> + Unpin,
    TX::Error: Error + Send + Sync + 'static,
{
    match serde_json::from_str(v) {
        Ok(cmd) => {
            handle_cmd(ctx.clone(), cmd)
                .fuse()
                .map(|v| serde_json::to_string(&v).expect("bad value"))
                .inspect(|v| tracing::trace!("Sending: {}", v))
                .map(Message::text)
                .map(Result::Ok)
                .forward(tx)
                .await?;
        }
        Err(e) => {
            tracing::warn!("Got invalid payload: {:?}", e);
            let error = CommandResponse::Error(e.to_string());
            let value = serde_json::to_string(&error)?;
            tx.send(Message::text(value)).await?
        }
    };
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpInformationResponse {
    ip: String,
}

pub async fn handle_ip_info(
    ip: Option<IpAddr>,
) -> Result<impl warp::Reply, Infallible> {
    Ok(warp::reply::json(&IpInformationResponse {
        ip: ip.unwrap().to_string(),
    }))
}

pub async fn handle_socket_info(
    ip: Option<SocketAddr>,
) -> Result<impl warp::Reply, Infallible> {
    Ok(warp::reply::json(&IpInformationResponse {
        ip: ip.unwrap().ip().to_string(),
    }))
}

pub async fn handle_relayer_info(
    ctx: Arc<RelayerContext>,
) -> Result<impl warp::Reply, Infallible> {
    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct RelayerInformationResponse {
        #[serde(flatten)]
        config: crate::config::WebbRelayerConfig,
    }
    // clone the original config, to update it with accounts.
    let mut config = ctx.config.clone();

    let _ = config
        .evm
        .values_mut()
        .filter(|v| v.account.is_none())
        .try_for_each(|v| {
            let key = SecretKey::from_bytes(v.private_key.as_bytes())?;
            let wallet = LocalWallet::from(key);
            v.account = Some(wallet.address());
            Result::<_, anyhow::Error>::Ok(())
        });
    Ok(warp::reply::json(&RelayerInformationResponse { config }))
}

pub async fn handle_leaves_cache(
    store: Arc<crate::store::sled::SledStore>,
    contract: Address,
) -> Result<impl warp::Reply, Infallible> {
    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct LeavesCacheResponse {
        leaves: Vec<H256>,
        last_queried_block: U64,
    }
    let leaves = store.get_leaves(contract).unwrap();
    let last_queried_block =
        store.get_last_deposit_block_number(contract).unwrap();
    Ok(warp::reply::json(&LeavesCacheResponse {
        leaves,
        last_queried_block,
    }))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Command {
    Substrate(SubstrateCommand),
    Evm(EvmCommand),
    Ping(),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubstrateCommand {}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvmCommand {
    /// one of the supported chains of this realyer
    pub chain: String,
    /// The target contract.
    pub contract: Address,
    /// Proof bytes
    pub proof: Bytes,
    /// Args...
    pub root: H256,
    pub nullifier_hash: H256,
    pub recipient: Address, // H160 ([u8; 20])
    pub relayer: Address,   // H160 (should be this realyer account)
    pub fee: U256,
    pub refund: U256,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CommandResponse {
    Pong(),
    Network(NetworkStatus),
    Withdraw(WithdrawStatus),
    Error(String),
    Unimplemented(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NetworkStatus {
    Connecting,
    Connected,
    Failed { reason: String },
    Disconnected,
    UnsupportedContract,
    UnsupportedChain,
    InvalidRelayerAddress,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WithdrawStatus {
    Sent,
    Submitted,
    Finalized {
        #[serde(rename = "txHash")]
        tx_hash: H256,
    },
    Valid,
    DroppedFromMemPool,
    Errored {
        code: i32,
        reason: String,
    },
}

pub fn handle_cmd<'a>(
    ctx: RelayerContext,
    cmd: Command,
) -> BoxStream<'a, CommandResponse> {
    use CommandResponse::*;
    match cmd {
        Command::Substrate(_) => stream::once(async {
            Unimplemented("Substrate based networks are not implemented yet.")
        })
        .boxed(),
        Command::Evm(evm) => handle_evm(ctx, evm),
        Command::Ping() => stream::once(async { Pong() }).boxed(),
    }
}

pub fn handle_evm<'a>(
    ctx: RelayerContext,
    cmd: EvmCommand,
) -> BoxStream<'a, CommandResponse> {
    use CommandResponse::*;
    let s = stream! {
        let requested_chain = cmd.chain.to_lowercase();
        let chain = match ctx.config.evm.get(&requested_chain) {
            Some(v) => v,
            None => {
                yield Network(NetworkStatus::UnsupportedChain);
                return;
            }
        };
        let supported_contracts: HashMap<_, _> = chain
            .contracts
            .iter()
            .cloned()
            .filter_map(|c| match c {
                crate::config::Contract::Anchor(c) => Some(c),
                _ => None,
            })
            .map(|c| (c.common.address, c))
            .collect();
        // get the contract configuration
        let contract_config = match supported_contracts.get(&cmd.contract) {
            Some(config) => config,
            None => {
                yield Network(NetworkStatus::UnsupportedContract);
                return;
            }
        };

        let wallet = match ctx.evm_wallet(&cmd.chain).await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("Misconfigured Network: {}", e);
                yield Error(format!("Misconfigured Network: {:?}", cmd.chain));
                return;
            }
        };
        // validate the relayer address first before trying
        // send the transaction.
        let relayer_address = wallet.address();
        if (cmd.relayer != relayer_address) {
            yield Network(NetworkStatus::InvalidRelayerAddress);
            return;
        }

        tracing::debug!(
            "Connecting to chain {:?} .. at {}",
            cmd.chain,
            chain.http_endpoint
        );
        yield Network(NetworkStatus::Connecting);
        let provider = match ctx.evm_provider(&cmd.chain).await {
            Ok(value) => {
                yield Network(NetworkStatus::Connected);
                value
            }
            Err(e) => {
                let reason = e.to_string();
                yield Network(NetworkStatus::Failed { reason });
                yield Network(NetworkStatus::Disconnected);
                return;
            }
        };

        let client = SignerMiddleware::new(provider, wallet);
        let client = Arc::new(client);
        let contract = AnchorContract::new(cmd.contract, client);
        let denomination = match contract.denomination().call().await {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("Misconfigured Contract Denomination: {}", e);
                yield Error(format!("Misconfigured Contract: {:?}", cmd.contract));
                return;
            }
        };
        // check the fee
        let expected_fee = calculate_fee(
            contract_config.withdraw_config.withdraw_fee_percentage,
            denomination,
        );
        let (_, unacceptable_fee) = U256::overflowing_sub(cmd.fee, expected_fee);
        if unacceptable_fee {
            tracing::error!("Received a fee lower than configuration");
            let msg = format!(
                "User sent a fee that is too low {} but expected {}",
                cmd.fee, expected_fee,
            );
            yield Error(msg);
            return;
        }

        let call = contract.withdraw(
            cmd.proof.to_vec(),
            cmd.root.to_fixed_bytes(),
            cmd.nullifier_hash.to_fixed_bytes(),
            cmd.recipient,
            cmd.relayer,
            cmd.fee,
            cmd.refund,
        );
        // Make a dry call, to make sure the transaction will go through successfully
        // to avoid wasting fees on invalid calls.
        match call.call().await {
            Ok(_) => {
                yield Withdraw(WithdrawStatus::Valid);
                tracing::debug!("Proof is valid");
            }
            Err(e) => {
                tracing::error!("Error Client sent an invalid proof: {}", e);
                let err = into_withdraw_error(e);
                yield Withdraw(err);
                return;
            }
        };
        tracing::trace!("About to send Tx to {:?} Chain", cmd.chain);
        let tx = match call.send().await {
            Ok(pending) => {
                yield Withdraw(WithdrawStatus::Sent);
                tracing::debug!("Tx is submitted and pending! {}", *pending);
                let result = pending.interval(Duration::from_millis(7000)).await;
                yield Withdraw(WithdrawStatus::Submitted);
                result
            }
            Err(e) => {
                tracing::error!("Error while sending Tx: {}", e);
                let err = into_withdraw_error(e);
                yield Withdraw(err);
                return;
            }
        };
        match tx {
            Ok(Some(receipt)) => {
                tracing::debug!("Finalized Tx #{}", receipt.transaction_hash);
                yield Withdraw(WithdrawStatus::Finalized {
                    tx_hash: receipt.transaction_hash,
                });
            }
            Ok(None) => {
                tracing::warn!("Transaction Dropped from Mempool!!");
                yield Withdraw(WithdrawStatus::DroppedFromMemPool);
            }
            Err(e) => {
                let reason = e.to_string();
                tracing::error!("Transaction Errored: {}", reason);
                yield Withdraw(WithdrawStatus::Errored { reason, code: 4 });
            }
        };
    };
    s.boxed()
}

fn into_withdraw_error<M: Middleware>(e: ContractError<M>) -> WithdrawStatus {
    // a poor man error parser
    // WARNING: **don't try this at home**.

    let msg = format!("{}", e);
    // split the error into words, lazily.
    let mut words = msg.split_whitespace();
    // skip until we find the "(code:" span
    let code = loop {
        if words.next() == Some("(code:") {
            // the next is the code.
            // example: "-32000,"
            let code: i32 = match words.next() {
                Some(val) => {
                    let mut v = val.to_string();
                    v.pop(); // remove ","
                    v.parse().unwrap_or(-1)
                }
                _ => -1, // unknown code
            };
            break code;
        }
    };

    let reason = loop {
        // skip until we find this
        if words.next() == Some("transaction:") {
            // next we need to collect all words in between "transaction:"
            // and "data:", that would be the error message.
            let msg: Vec<_> = words
                .skip(1) // word "revert"
                .take_while(|v| *v != "data:")
                .collect();
            let mut reason = msg.join(" ");
            reason.pop(); // remove the "," at the end.
            break reason;
        }
    };
    WithdrawStatus::Errored { reason, code }
}

fn calculate_fee(fee_percent: f64, principle: U256) -> U256 {
    let mill_fee = (fee_percent * 1_000_000.0) as u32;
    let mill_u256: U256 = principle * (mill_fee);
    let fee_u256: U256 = mill_u256 / (1_000_000);
    fee_u256
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn percent_fee() {
        let submitted_value =
            U256::from_dec_str("5000000000000000").ok().unwrap();
        let expected_fee = U256::from_dec_str("250000000000000").ok().unwrap();
        let withdraw_fee_percent_dec = 0.05f64;
        let formatted_fee =
            calculate_fee(withdraw_fee_percent_dec, submitted_value);

        assert_eq!(expected_fee, formatted_fee);
    }
}
