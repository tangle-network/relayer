use core::fmt;

use tangle_subxt::subxt::{
    self,
    ext::scale_encode::EncodeAsFields,
    tx::{Payload, TxPayload},
};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct TypeErasedStaticTxPayload {
    pallet_name: String,
    call_name: String,
    #[serde(with = "serde_bytes")]
    tx_data: Vec<u8>,
    validation_hash: [u8; 32],
}

impl TypeErasedStaticTxPayload {
    pub fn tx_data(&self) -> &[u8] {
        self.tx_data.as_slice()
    }
}

impl std::fmt::Debug for TypeErasedStaticTxPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypeErasedStaticTxPayload")
            .field("pallet_name", &self.pallet_name)
            .field("call_name", &self.call_name)
            .field("tx_data", &hex::encode(&self.tx_data))
            .field("validation_hash", &hex::encode(self.validation_hash))
            .finish()
    }
}

impl fmt::Display for TypeErasedStaticTxPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}.{}({})",
            self.pallet_name,
            self.call_name,
            hex::encode(&self.tx_data)
        )
    }
}

impl<'a, CallData: EncodeAsFields>
    TryFrom<(&'a subxt::Metadata, Payload<CallData>)>
    for TypeErasedStaticTxPayload
{
    type Error = super::Error;
    fn try_from(
        (metadata, payload): (&'a subxt::Metadata, Payload<CallData>),
    ) -> Result<Self, Self::Error> {
        let details = payload
            .validation_details()
            .ok_or_else(|| Self::Error::MissingValidationDetails)?;
        let mut tx_data = Vec::new();
        payload.encode_call_data_to(metadata, &mut tx_data)?;
        Ok(Self {
            pallet_name: details.pallet_name.to_owned(),
            call_name: details.call_name.to_owned(),
            tx_data,
            validation_hash: details.hash,
        })
    }
}

impl TxPayload for TypeErasedStaticTxPayload {
    fn encode_call_data_to(
        &self,
        _metadata: &subxt::Metadata,
        out: &mut Vec<u8>,
    ) -> Result<(), tangle_subxt::subxt::Error> {
        out.clone_from(&self.tx_data);
        Ok(())
    }
}
