use std::any::Any;

use eth_types::eth2::{FinalizedHeaderUpdate, ExtendedBeaconBlockHeader};
use webb::substrate::{subxt::{self, OnlineClient, PolkadotConfig, dynamic::{Value, DecodedValue}}, scale::Decode};
use webb_proposals::TypedChainId;
use webb_relayer_utils::Error;
use webb::substrate::subxt::metadata::DecodeWithMetadata;

pub async fn setup_api() -> Result<OnlineClient<PolkadotConfig>, Error> {
    let api = OnlineClient::<PolkadotConfig>::new().await?;
    Ok(api)
}

async fn finalized_beacon_block_slot(typed_chain_id: TypedChainId) -> Result<u64, Error> {
    let api = setup_api().await?;

    let storage_address = subxt::dynamic::storage(
        "Eth2Client",
        "FinalizedBeaconHeader",
        vec![
            Value::from_bytes(&typed_chain_id.chain_id().to_be_bytes()),
        ],
    );
    let finalized_beacon_header_value: DecodedValue = api
        .storage()
        .fetch_or_default(&storage_address, None)
        .await?;

    Ok(0)
}

pub async fn get_last_eth2_slot_on_tangle(typed_chain_id: TypedChainId) -> Result<u64, Error> {
    let api = setup_api().await?;

    let storage_address = subxt::dynamic::storage(
        "Eth2Client",
        "FinalizedHeaderUpdate",
        vec![
            Value::from_bytes(&typed_chain_id.chain_id().to_be_bytes()),
        ],
    );
    let finalized_header_update: DecodedValue = api
        .storage()
        .fetch_or_default(&storage_address, None)
        .await?;

    Ok(0)
}
