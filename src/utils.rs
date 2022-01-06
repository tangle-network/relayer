use std::fmt;

use webb::substrate::subxt;
use webb::substrate::subxt::sp_core::storage::StorageChangeSet;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ClickableLink<'a> {
    text: &'a str,
    url: &'a str,
}

impl<'a> ClickableLink<'a> {
    /// Create a new link with a name and target url.
    pub fn new(text: &'a str, url: &'a str) -> Self {
        Self { text, url }
    }
}

impl fmt::Display for ClickableLink<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "\u{1b}]8;;{}\u{1b}\\{}\u{1b}]8;;\u{1b}\\",
            self.url, self.text
        )
    }
}

pub fn change_set_to_events<C: subxt::Config, E: subxt::Event>(
    change_set: StorageChangeSet<C::Hash>,
    decoder: &subxt::EventsDecoder<C>,
) -> Vec<(C::Hash, E)> {
    let current_block_hash = change_set.block;
    change_set
        .changes
        .into_iter()
        .filter_map(|(_key, change)| {
            let bytes = match change {
                Some(change) => change.0,
                None => return None,
            };
            let decoded = decoder.decode_events(&mut bytes.as_slice());
            match decoded {
                Ok(events) => Some(events),
                Err(err) => {
                    tracing::warn!("Failed to decode events: {:?}", err);
                    None
                }
            }
        })
        .flatten()
        .filter_map(|(phase, raw_event)| {
            let is_apply_extrinsic =
                matches!(phase, subxt::Phase::ApplyExtrinsic(_));
            if is_apply_extrinsic {
                Some((current_block_hash, raw_event))
            } else {
                None
            }
        })
        .filter_map(|(block, raw)| match raw.as_event::<E>() {
            Ok(event) => event.map(|event| (block, event)),
            Err(_) => None,
        })
        .collect()
}
