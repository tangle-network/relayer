// Copyright (C) 2022-2024 Webb Technologies Inc.
//
// Tangle is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Tangle is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should receive a copy of the GNU General Public License
// If not, see <http://www.gnu.org/licenses/>.

use std::time::Duration;

use webb::evm::ethers::providers::ProviderError;
use webb::evm::ethers::providers::{JsonRpcError, RetryPolicy};

/// Implements [RetryPolicy] that will retry requests that errored with
/// status code 429 i.e. TOO_MANY_REQUESTS
///
/// Infura often fails with a `"header not found"` rpc error which is apparently linked to load
/// balancing, which are retried as well.
///
// Copied and modifed from: https://github.com/gakonst/ethers-rs/blob/b6c2d89c379ebc4573c269b5dc80c8a9fd2e9a49/ethers-providers/src/rpc/transports/retry.rs#L364
#[derive(Debug)]
pub struct WebbHttpRetryPolicy {
    err_regex: regex::Regex,
}

impl WebbHttpRetryPolicy {
    pub fn new() -> Self {
        Self {
            err_regex: regex::Regex::new(
                r"(?mixU)\b(?:rate|limit|429|Too \s Many \s Requests)\b",
            )
            .expect("Valid Regex"),
        }
    }

    pub fn boxed() -> Box<Self> {
        Box::new(Self::new())
    }
}

fn should_retry_json_rpc_error(err: &JsonRpcError) -> bool {
    let JsonRpcError { code, message, .. } = err;
    // alchemy throws it this way
    if *code == 429 {
        return true;
    }

    // This is an infura error code for `exceeded project rate limit`
    if *code == -32005 {
        return true;
    }

    // alternative alchemy error for specific IPs
    if *code == -32016 && message.contains("rate limit") {
        return true;
    }

    match message.as_str() {
        // this is commonly thrown by infura and is apparently a load balancer issue, see also <https://github.com/MetaMask/metamask-extension/issues/7234>
        "header not found" => true,
        // also thrown by infura if out of budget for the day and ratelimited
        "daily request count exceeded, request rate limited" => true,
        _ => false,
    }
}

// check json rpc error in serde error
fn should_retry_json_rpc_error_from_serde(
    err: &serde_json::Error,
    err_regex: regex::Regex,
) -> bool {
    // some providers send invalid JSON RPC in the error case (no `id:u64`), but the
    // text should be a `JsonRpcError`
    #[derive(serde::Deserialize)]
    struct Resp {
        error: JsonRpcError,
    }

    if let Ok(resp) = serde_json::from_str::<Resp>(&err.to_string()) {
        return should_retry_json_rpc_error(&resp.error);
    }

    let err_text = err.to_string().to_lowercase();

    // last resort, some providers send the error message in the text
    // and the text itself is not a valid json response either.
    // check if we have the word "rate", or "limit" in the error message
    // and if so, we should retry

    let should_retry = err_regex.is_match(&err_text)
        || matches!(err_text.as_str(), "expected value at line 1 column 1");

    tracing::event!(
        target: webb_relayer_utils::probe::TARGET,
        tracing::Level::DEBUG,
        kind = %webb_relayer_utils::probe::Kind::Retry,
        should_retry = should_retry,
        error = %err_text,
    );
    should_retry
}

impl RetryPolicy<ProviderError> for WebbHttpRetryPolicy {
    fn should_retry(&self, error: &ProviderError) -> bool {
        tracing::debug!("should_retry: {:?}", error);
        match error {
            ProviderError::HTTPError(err) => {
                err.status() == Some(http::StatusCode::TOO_MANY_REQUESTS)
            }
            ProviderError::JsonRpcClientError(err) => {
                if let Some(e) = err.as_error_response() {
                    return should_retry_json_rpc_error(e);
                };

                tracing::debug!("Error source: {:?}", err.source());
                if let Some(e) = err.as_serde_error() {
                    // let new_err = SerdeJson::from(err).into();
                    tracing::debug!("Serede error: {:?}", e);
                    return should_retry_json_rpc_error_from_serde(
                        e,
                        self.err_regex.clone(),
                    );
                }

                false
            }
            ProviderError::EnsError(_) => true,
            ProviderError::EnsNotOwned(_) => true,
            ProviderError::HexError(_) => false,
            ProviderError::CustomError(_) => false,
            ProviderError::UnsupportedRPC => false,
            ProviderError::UnsupportedNodeClient => false,
            ProviderError::SignerUnavailable => false,

            ProviderError::SerdeJson(err) => {
                should_retry_json_rpc_error_from_serde(
                    err,
                    self.err_regex.clone(),
                )
            }
        }
    }

    fn backoff_hint(&self, error: &ProviderError) -> Option<Duration> {
        const DEFAULT_BACKOFF: Duration = Duration::from_secs(5);

        if let ProviderError::JsonRpcClientError(err) = error {
            let json_rpc_error = err.as_error_response();
            if let Some(json_rpc_error) = json_rpc_error {
                if let Some(data) = &json_rpc_error.data {
                    // if daily rate limit exceeded, infura returns the requested backoff in the error
                    // response
                    let Some(backoff_seconds) = data.get("rate").and_then(|v| v.get("backoff_seconds")) else {
                        return Some(DEFAULT_BACKOFF);
                    };
                    // infura rate limit error
                    if let Some(seconds) = backoff_seconds.as_u64() {
                        return Some(Duration::from_secs(seconds));
                    }
                    if let Some(seconds) = backoff_seconds.as_f64() {
                        return Some(Duration::from_secs(seconds as u64 + 1));
                    }
                }
            }
        }

        // A default value of 5s
        Some(DEFAULT_BACKOFF)
    }
}
