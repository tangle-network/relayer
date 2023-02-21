use webb::substrate::subxt;
use webb_event_watcher_traits::SubstrateEventWatcher;

/// Module for handling DKG Public Key Changed event.
mod public_key_changed_handler;

/// The DKG Metadata watcher watches for the events from the Dkg Pallet.
#[derive(Clone, Debug)]
pub struct DKGMetadataWatcher;

#[async_trait::async_trait]
impl SubstrateEventWatcher<subxt::PolkadotConfig> for DKGMetadataWatcher {
    const TAG: &'static str = "DKG Metadata Pallet Watcher";

    const PALLET_NAME: &'static str = "DKG";

    type Client = subxt::OnlineClient<subxt::PolkadotConfig>;

    type Store = webb_relayer_store::SledStore;
}
