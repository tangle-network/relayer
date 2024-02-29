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

use std::collections::HashMap;

/// The default port the relayer will listen on. Defaults to 9955.
pub const fn relayer_port() -> u16 {
    9955
}
/// Leaves watcher is set to `true` by default.
pub const fn enable_leaves_watcher() -> bool {
    true
}
/// Data query access is set to `true` by default.
pub const fn enable_data_query() -> bool {
    true
}
/// The maximum events per step is set to `100` by default.
pub const fn max_blocks_per_step() -> u64 {
    500
}
/// The print progress interval is set to `7_000` by default.
pub const fn print_progress_interval() -> u64 {
    7_000
}

/// The default unlisted assets.
pub fn unlisted_assets() -> HashMap<String, crate::UnlistedAssetConfig> {
    HashMap::from_iter([
        (
            String::from("tTNT"),
            crate::UnlistedAssetConfig {
                name: String::from("Test Tangle Network Token"),
                decimals: 18,
                price: 0.10,
            },
        ),
        (
            String::from("TNT"),
            crate::UnlistedAssetConfig {
                name: String::from("Tangle Network Token"),
                decimals: 18,
                price: 0.10,
            },
        ),
        // Orbit Network
        (
            String::from("ORBt"),
            crate::UnlistedAssetConfig {
                name: String::from("Webb Orbit Network Token"),
                decimals: 18,
                price: 0.10,
            },
        ),
    ])
}
