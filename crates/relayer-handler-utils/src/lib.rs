// Copyright 2022 Webb Technologies Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![allow(clippy::large_enum_variant)]
#![allow(missing_docs)]

use serde::{Deserialize, Deserializer, Serialize};
use webb::evm::ethers::abi::Address;
use webb::evm::ethers::prelude::I256;

use webb::evm::ethers::types::Bytes;
use webb::evm::ethers::types::U256;

use webb_relayer_tx_relay_utils::{
    MaspRelayTransaction, VAnchorRelayTransaction,
};

/// Representation for IP address response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IpInformationResponse {
    pub ip: String,
}

/// A wrapper type around [`I256`] that implements a correct way for [`Serialize`] and [`Deserialize`].
///
/// This supports the signed integer hex values that are not originally supported by the [`I256`] type.
#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct WebbI256(pub I256);

impl<'de> Deserialize<'de> for WebbI256 {
    fn deserialize<D>(deserializer: D) -> Result<WebbI256, D::Error>
    where
        D: Deserializer<'de>,
    {
        let i128_str = String::deserialize(deserializer)?;
        let i128_val =
            I256::from_hex_str(&i128_str).map_err(serde::de::Error::custom)?;
        Ok(WebbI256(i128_val))
    }
}
/// A wrapper type around [`i128`] that implements a correct way for [`Serialize`] and [`Deserialize`].
///
/// This supports the signed integer hex values that are not originally supported by the [`i128`] type.
#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct WebbI128(pub i128);

impl<'de> Deserialize<'de> for WebbI128 {
    fn deserialize<D>(deserializer: D) -> Result<WebbI128, D::Error>
    where
        D: Deserializer<'de>,
    {
        let i128_str = String::deserialize(deserializer)?;
        let value = i128::from_str_radix(&i128_str, 16)
            .map_err(serde::de::Error::custom)?;
        Ok(WebbI128(value))
    }
}

/// The command type for EVM vanchor transactions
pub type EvmVanchorCommand = EvmCommandType<
    Bytes,    // Proof bytes
    Bytes,    // Roots format
    U256,     // Element type
    Address,  // Account identifier
    U256,     // Balance type
    WebbI256, // Signed amount type
    Address,  // Token Address
>;

/// Enumerates the supported protocols for relaying transactions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EvmCommandType<P, R, E, I, B, A, T> {
    VAnchor(VAnchorRelayTransaction<P, R, E, I, B, A, T>),
    MaspVanchor(MaspRelayTransaction<P, R, E, I, B, A, T>),
}
