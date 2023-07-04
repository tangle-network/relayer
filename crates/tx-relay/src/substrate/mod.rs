use sp_core::sr25519::Pair;
use webb::substrate::subxt::tx::PairSigner;
use webb::substrate::subxt::{OnlineClient, PolkadotConfig};
use webb::substrate::tangle_runtime::api;

pub mod fees;
/// Substrate Variable Anchor Transactional Relayer.
pub mod vanchor;

fn wei_to_gwei(wei: u128) -> f64 {
    (wei / (10 ^ 9)) as f64
}

async fn balance(
    client: OnlineClient<PolkadotConfig>,
    signer: PairSigner<PolkadotConfig, Pair>,
) -> webb_relayer_utils::Result<u128> {
    let account = api::storage().system().account(signer.account_id());
    let balance = client
        .storage()
        .at_latest()
        .await?
        .fetch(&account)
        .await?
        .ok_or(webb_relayer_utils::Error::ReadSubstrateStorageError)?;
    Ok(balance.data.free)
}
