use std::time::Duration;

use webb::evm::ethers::providers::HttpClientError;
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
                r"(?mixU)\b(?:rate|limit|429|Too Many Requests)\b",
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

impl RetryPolicy<HttpClientError> for WebbHttpRetryPolicy {
    fn should_retry(&self, error: &HttpClientError) -> bool {
        match error {
            HttpClientError::ReqwestError(err) => {
                err.status() == Some(http::StatusCode::TOO_MANY_REQUESTS)
            }
            HttpClientError::JsonRpcError(err) => {
                should_retry_json_rpc_error(err)
            }
            HttpClientError::SerdeJson { text, .. } => {
                // some providers send invalid JSON RPC in the error case (no `id:u64`), but the
                // text should be a `JsonRpcError`
                #[derive(serde::Deserialize)]
                struct Resp {
                    error: JsonRpcError,
                }

                if let Ok(resp) = serde_json::from_str::<Resp>(text) {
                    return should_retry_json_rpc_error(&resp.error);
                }

                let err_text = text.to_lowercase();
                // last resort, some providers send the error message in the text
                // and the text itself is not a valid json response either.
                // check if we have the word "rate", or "limit" in the error message
                // and if so, we should retry
                let should_retry = self.err_regex.is_match(&err_text);
                tracing::event!(
                    target: webb_relayer_utils::probe::TARGET,
                    tracing::Level::DEBUG,
                    kind = %webb_relayer_utils::probe::Kind::Retry,
                    should_retry = should_retry,
                    error = %err_text,
                );
                should_retry
            }
        }
    }

    fn backoff_hint(&self, error: &HttpClientError) -> Option<Duration> {
        const DEFAULT_BACKOFF: Duration = Duration::from_secs(5);

        if let HttpClientError::JsonRpcError(JsonRpcError { data, .. }) = error
        {
            let data = data.as_ref()?;

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

        // A default value of 5s
        Some(DEFAULT_BACKOFF)
    }
}
