use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize)]
pub struct EtherscanApiKey(String);

impl std::fmt::Debug for EtherscanApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("EtherscanApiKey").finish()
    }
}

impl From<String> for EtherscanApiKey {
    fn from(api_key: String) -> Self {
        EtherscanApiKey(api_key)
    }
}

impl std::ops::Deref for EtherscanApiKey {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de> Deserialize<'de> for EtherscanApiKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct EtherscanApiKeyVisitor;
        impl<'de> serde::de::Visitor<'de> for EtherscanApiKeyVisitor {
            type Value = String;

            fn expecting(
                &self,
                formatter: &mut std::fmt::Formatter,
            ) -> std::fmt::Result {
                formatter.write_str(
                    "Etherscan api key or an env var containing a etherscan api key in it",
                )
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if value.starts_with('$') {
                    // env
                    let var = value.strip_prefix('$').unwrap_or(value);
                    tracing::trace!("Reading {} from env", var);
                    let val = std::env::var(var).map_err(|e| {
                        serde::de::Error::custom(format!(
                            "error while loading this env {var}: {e}",
                        ))
                    })?;
                    return Ok(val);
                }
                Ok(value.to_string())
            }
        }

        let etherscan_api_key =
            deserializer.deserialize_str(EtherscanApiKeyVisitor)?;
        Ok(Self(etherscan_api_key))
    }
}
