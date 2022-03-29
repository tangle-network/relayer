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
use derive_more::Display;
pub const TARGET: &str = "webb_probe";

/// The Kind of the Probe.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// When the Lifecycle of the Relayer changes, like starting or shutting down.
    #[display(fmt = "lifecycle")]
    Lifecycle,
    /// Relayer Sync state on a specific chain/node.
    #[display(fmt = "sync")]
    Sync,
    /// Relaying a transaction state.
    #[display(fmt = "relay_tx")]
    RelayTx,
}
