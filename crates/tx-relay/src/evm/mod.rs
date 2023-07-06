use ethereum_types::U256;
use webb::evm::ethers;

/// For Fees calculation.
pub mod fees;
/// Variable Anchor transaction relayer.
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
