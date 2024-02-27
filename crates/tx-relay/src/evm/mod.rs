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

use ethereum_types::U256;
use webb::evm::ethers;

/// For Fees calculation.
pub mod fees;
/// MASP vanchor transaction relaying.
#[cfg(feature = "masp-tx-relaying")]
pub mod masp_vanchor;
/// Variable Anchor transaction relaying.
pub mod vanchor;

fn wei_to_gwei(wei: U256) -> f64 {
    ethers::utils::format_units(wei, "gwei")
        .and_then(|gas| {
            gas.parse::<f64>()
                // TODO: this error is pointless as it is silently dropped
                .map_err(|_| ethers::utils::ConversionError::ParseOverflow)
        })
        .unwrap_or_default()
}
