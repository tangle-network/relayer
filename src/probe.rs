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
