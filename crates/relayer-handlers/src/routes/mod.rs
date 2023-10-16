use ethereum_types::H512;
use serde::{Deserialize, Serialize};

/// Module for handling encrypted commitment leaves API
pub mod encrypted_outputs;

/// Module for handle commitment leaves API
pub mod leaves;

/// Module for handling relayer metric API
pub mod metric;

/// Module for handling relayer info API
pub mod info;

/// Module for handling fee info API
pub mod fee_info;

/// Module for handling transaction status API
pub mod transaction_status;

/// Module for handling private tx withdraw API
pub mod private_tx_withdraw;

/// Module for handling masp private tx withdrawal API
pub mod masp_tx_relaying;

/// A (half-open) range bounded inclusively below and exclusively above
/// (`start..end`).
///
/// The range `start..end` contains all values with `start <= x < end`.
/// It is empty if `start >= end`.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionalRangeQuery {
    /// The lower bound of the range (inclusive).
    ///
    /// default: Zero
    #[serde(default = "default_zero")]
    pub start: Option<u32>,
    /// The upper bound of the range (exclusive).
    ///
    /// default: `u32::MAX`
    #[serde(default = "default_u32_max")]
    pub end: Option<u32>,
}

impl Default for OptionalRangeQuery {
    fn default() -> Self {
        Self {
            start: default_zero(),
            end: default_u32_max(),
        }
    }
}

impl From<OptionalRangeQuery> for core::ops::Range<u32> {
    fn from(range: OptionalRangeQuery) -> Self {
        let start = range
            .start
            .or_else(default_zero)
            .expect("start is not None");
        let end = range.end.or_else(default_u32_max).expect("end is not None");
        start..end
    }
}

const fn default_zero() -> Option<u32> {
    Some(0)
}

const fn default_u32_max() -> Option<u32> {
    Some(u32::MAX)
}

/// Success response for withdrawal tx relaying API request.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawTxSuccessResponse {
    status: String,
    message: String,
    item_key: H512,
}

/// Failure response for withdrawal tx relaying API request.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WithdrawTxFailureResponse {
    status: String,
    message: String,
    reason: String,
}

/// Withdrawal tx relaying API request response
#[derive(Debug, Serialize)]
#[serde(untagged)]
#[serde(rename_all = "camelCase")]
pub enum WithdrawTxResponse {
    /// Success response for withdrawal tx API request.
    Success(WithdrawTxSuccessResponse),
    /// Failure response for withdrawal tx API request.
    Failure(WithdrawTxFailureResponse),
}
