use serde::{Deserialize, Serialize};

/// Module for handling encrypted commitment leaves API
pub mod encrypted_leaves;

/// Module for handle commitment leaves API
pub mod leaves;

/// Module for handling relayer metric API
pub mod metric;

/// Module for handling relayer info API
pub mod info;

// Unsupported feature response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct UnsupportedFeature {
    message: String,
}

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
        let start = range.start.unwrap_or(0);
        let end = range.end.unwrap_or(u32::MAX);
        start..end
    }
}

const fn default_zero() -> Option<u32> {
    Some(0)
}

const fn default_u32_max() -> Option<u32> {
    Some(u32::MAX)
}
