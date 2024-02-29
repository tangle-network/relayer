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
