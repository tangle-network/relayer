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
//
use std::fmt;
use tiny_keccak::{Hasher, Keccak};

/// Represents a clickable link containing text and url
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ClickableLink<'a> {
    text: &'a str,
    url: &'a str,
}

impl<'a> ClickableLink<'a> {
    /// Create a new link with a name and target URL, helpful to print clickable links in the terminal.
    pub fn new(text: &'a str, url: &'a str) -> Self {
        Self { text, url }
    }
}

impl fmt::Display for ClickableLink<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "\u{1b}]8;;{}\u{1b}\\{}\u{1b}]8;;\u{1b}\\",
            self.url, self.text
        )
    }
}

/// Computes the numeric "chain_id" from string one.
/// This is only needed for Cosmos SDK blockchains since
/// their "chain_id"s are string(eg: "juno-1")
/// Rule:
///   1. Hash the "chain_id" to get 32-length bytes array
///       eg: keccak256("juno-1") => 4c22bf61f15534242ee9dba16dceb4c976851b1788680fb5ee2a7b568a294d21
///   2. Slice the last 4 bytes & convert it to `u32` numeric value
///       eg: 8a294d21(hex) -> 2317962529(decimal)
pub fn compute_cosmwasm_chain_id(chain_id_str: &str) -> u32 {
    let mut keccak = Keccak::v256();
    keccak.update(chain_id_str.as_bytes());

    let mut output = [0u8; 32];
    keccak.finalize(&mut output);

    let last_4_bytes = &output[28..];

    let mut buf = [0u8; 4];
    buf[0..4].copy_from_slice(last_4_bytes);
    u32::from_be_bytes(buf)
}
