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
