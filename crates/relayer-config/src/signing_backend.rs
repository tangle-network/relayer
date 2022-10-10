use webb_relayer_types::private_key::PrivateKey;

use super::*;

/// Enumerates the supported different signing backends configurations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ProposalSigningBackendConfig {
    /// Uses an already running and configured DKG Node for signing proposals.
    #[serde(rename = "DKGNode")]
    DkgNode(DkgNodeProposalSigningBackendConfig),
    /// Uses the Private Key of the current Governor to sign proposals.
    Mocked(MockedProposalSigningBackendConfig),
}

/// DKGNodeSigningBackendConfig represents the configuration for the DKGNode signing backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct DkgNodeProposalSigningBackendConfig {
    /// The name of the DKG Node that this contract will use.
    ///
    /// Must be defined in the config.
    pub node: String,
}

/// MockedSigningBackendConfig represents the configuration for the Mocked signing backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct MockedProposalSigningBackendConfig {
    /// The private key of the current Governor.
    #[serde(skip_serializing)]
    pub private_key: PrivateKey,
}
