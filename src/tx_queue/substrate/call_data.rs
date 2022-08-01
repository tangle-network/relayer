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
use parking_lot::RwLock;
use std::sync::Arc;
use webb::substrate::subxt::{Call, Metadata};
// call data bytes (pallet u8, call u8, call params).
pub fn encode_call_data<C: Call>(
    metadata: Arc<RwLock<Metadata>>,
    call: C,
) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let metadata = metadata.read();
    let pallet = metadata.pallet(C::PALLET)?;
    bytes.push(pallet.index());
    bytes.push(pallet.call_index::<C>()?);
    call.encode_to(&mut bytes);
    Ok(bytes)
}
