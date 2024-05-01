use webb_relayer_types::private_key::PrivateKey;

use super::*;

/// Enumerates the supported different signing backends configurations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProposalSigningBackendConfig {
    /// Uses signing rules contract to vote and submit proposals for signing.
    Contract(SigningRulesBackendConfig),
    /// Uses the Private Key of the current Governor to sign proposals.
    Mocked(MockedProposalSigningBackendConfig),
}

/// SigningRulesBackendConfig represents the configuration for the DKG signing backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "kebab-case"))]
pub struct SigningRulesBackendConfig {
    /// Chain ID of the chain where the signing rules contract is deployed.
    pub chain_id: u32,
    /// The address of this contract on this chain.
    pub address: Address,
    /// Phase one Job Id
    pub phase_one_job_id: u64,
}

/// MockedSigningBackendConfig represents the configuration for the Mocked signing backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "kebab-case"))]
pub struct MockedProposalSigningBackendConfig {
    /// The private key of the current Governor.
    #[serde(skip_serializing)]
    pub private_key: PrivateKey,
}
