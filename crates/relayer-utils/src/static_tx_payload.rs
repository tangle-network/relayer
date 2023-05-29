use core::fmt;

use webb::substrate::{
    scale::Encode,
    subxt::tx::{StaticTxPayload, TxPayload},
};

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct TypeErasedStaticTxPayload {
    pub pallet_name: String,
    pub call_name: String,
    #[serde(with = "serde_bytes")]
    pub call_data: Vec<u8>,
}

impl std::fmt::Debug for TypeErasedStaticTxPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TypeErasedStaticTxPayload")
            .field("pallet_name", &self.pallet_name)
            .field("call_name", &self.call_name)
            .field("call_data", &hex::encode(&self.call_data))
            .finish()
    }
}

impl fmt::Display for TypeErasedStaticTxPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{}.{}({})",
            self.pallet_name,
            self.call_name,
            hex::encode(&self.call_data)
        )
    }
}

impl<CallData: Encode> TryFrom<StaticTxPayload<CallData>>
    for TypeErasedStaticTxPayload
{
    type Error = super::Error;
    fn try_from(
        payload: StaticTxPayload<CallData>,
    ) -> Result<Self, Self::Error> {
        let details = payload
            .validation_details()
            .ok_or_else(|| Self::Error::MissingValidationDetails)?;
        let call_data = payload.call_data().encode();
        Ok(Self {
            pallet_name: details.pallet_name.to_owned(),
            call_name: details.call_name.to_owned(),
            call_data,
        })
    }
}

impl TxPayload for TypeErasedStaticTxPayload {
    fn encode_call_data_to(
        &self,
        metadata: &webb::substrate::subxt::Metadata,
        out: &mut Vec<u8>,
    ) -> Result<(), webb::substrate::subxt::Error> {
        let pallet = metadata.pallet(&self.pallet_name)?;
        let pallet_index = pallet.index();
        let call_index = pallet.call_index(&self.call_name)?;

        pallet_index.encode_to(out);
        call_index.encode_to(out);
        out.extend_from_slice(&self.call_data);
        Ok(())
    }
}
