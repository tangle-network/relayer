use webb_relayer_types::private_key::PrivateKey;

use super::*;

/// Enumerates the supported different signing backends configurations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProposalSigningBackendConfig {
    /// Uses signing rules contract to vote and submit proposals for signing.
    Dkg(DkgProposalSigningBackendConfig),
    /// Uses the Private Key of the current Governor to sign proposals.
    Mocked(MockedProposalSigningBackendConfig),
}

/// DkgProposalSigningBackendConfig represents the configuration for the DKG signing backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "kebab-case"))]
pub struct DkgProposalSigningBackendConfig {
    /// The address of this contract on this chain.
    pub address: Address,
    /// Phase1 Job Id
    pub phase1_job_id: [u8; 32],
}

/// MockedSigningBackendConfig represents the configuration for the Mocked signing backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all(serialize = "camelCase", deserialize = "kebab-case"))]
pub struct MockedProposalSigningBackendConfig {
    /// The private key of the current Governor.
    #[serde(skip_serializing)]
    pub private_key: PrivateKey,
}
