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

use derive_more::Display;
/// Target for logger
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
    /// Signing Backend updates.
    #[display(fmt = "signing_backend")]
    SigningBackend,
    /// Signature Bridge on a specific chain/node.
    #[display(fmt = "signature_bridge")]
    SignatureBridge,
    /// Relayer Transaction Queue state on a specific chain/node.
    #[display(fmt = "tx_queue")]
    TxQueue,
    /// Relayer Private transaction state on a specific chain/node.
    #[display(fmt = "private_tx")]
    PrivateTx,
    /// Relayer Leaves Store state on a specific chain/node.
    #[display(fmt = "leaves_store")]
    LeavesStore,
    /// When the Relayer sees a new Merkle Tree insertion event on a specific chain/node.
    #[display(fmt = "mt_insert")]
    MerkleTreeInsertion,
    /// Relayer Encrypted Output Store state on a specific chain/node.
    #[display(fmt = "encrypted_outputs_store")]
    EncryptedOutputStore,
    /// When the relayer will retry to do something.
    #[display(fmt = "retry")]
    Retry,
}
