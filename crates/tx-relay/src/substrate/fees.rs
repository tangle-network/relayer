use serde::Serialize;
use webb_proposals::TypedChainId;
use webb_relayer_context::RelayerContext;
use webb_relayer_utils::Result;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SubstrateFeeInfo {}

pub async fn get_substrate_fee_info(
    chain_id: TypedChainId,
    ctx: &RelayerContext,
) -> Result<SubstrateFeeInfo> {
    Ok(SubstrateFeeInfo {})
}
