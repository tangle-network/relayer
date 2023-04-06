//! Webb Chains Information
//!
//! This crate contains the information about the chains that are supported by the relayer.
//! The information is used to generate the `chains.rs` file and could be used by
//! the relayer to get the information about the chains.

include!(concat!(env!("OUT_DIR"), "/chains.rs"));

/// Get the chains information.
#[must_use]
#[inline]
pub fn chains_info() -> &'static chains::ChainsInfo {
    &chains::CHAINS_INFO
}

/// Get the chain information by the chain identifier.
#[must_use]
pub fn chain_info_by_chain_id(
    chain_id: u64,
) -> Option<&'static chains::ChainInfo> {
    chains::CHAINS_INFO
        .binary_search_by_key(&chain_id, |(id, _)| *id)
        .ok()
        .map(|index| &chains::CHAINS_INFO[index])
        .map(|(_, info)| info)
}
