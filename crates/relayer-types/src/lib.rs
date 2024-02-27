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

use std::sync::Arc;
use webb::evm::ethers::{prelude::TimeLag, providers};
use webb_relayer_utils::multi_provider::MultiProvider;

pub mod etherscan_api;
pub mod mnemonic;
pub mod private_key;
pub mod rpc_client;
pub mod rpc_url;
pub mod suri;

/// Ethereum TimeLag client using Ethers, that includes a retry strategy.
pub type EthersTimeLagClient = TimeLag<
    Arc<
        providers::Provider<
            providers::RetryClient<MultiProvider<providers::Http>>,
        >,
    >,
>;

pub type EthersClient =
    providers::Provider<providers::RetryClient<MultiProvider<providers::Http>>>;
