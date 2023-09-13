use std::str::FromStr;

use serde::Deserialize;
use subxt_signer::sr25519::Keypair as Sr25519Pair;

/// [`Substrate Uri`](https://polkadot.js.org/docs/keyring/start/suri/)
#[derive(Clone)]
pub struct Suri(pub Sr25519Pair);

impl std::fmt::Debug for Suri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Substrate Uri").finish()
    }
}

impl From<Suri> for Sr25519Pair {
    fn from(suri: Suri) -> Self {
        suri.0
    }
}

impl std::ops::Deref for Suri {
    type Target = Sr25519Pair;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Suri {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct PrivateKeyVistor;
        impl<'de> serde::de::Visitor<'de> for PrivateKeyVistor {
            type Value = Sr25519Pair;

            fn expecting(
                &self,
                formatter: &mut std::fmt::Formatter,
            ) -> std::fmt::Result {
                formatter.write_str(
                    "hex string, dervation path or an env var containing a hex string in it",
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
                    parse_suri(&val)
                } else if value.starts_with('>') {
                    todo!("Implement command execution to extract the private key")
                } else {
                    parse_suri(value)
                }
            }
        }

        let secret = deserializer.deserialize_str(PrivateKeyVistor)?;
        Ok(Self(secret))
    }
}

fn parse_suri<E>(val: &str) -> Result<Sr25519Pair, E>
where
    E: serde::de::Error,
{
    use subxt_signer::bip39::Mnemonic;
    use subxt_signer::SecretUri;

    let secret_uri = SecretUri::from_str(val);
    if let Ok(secret_uri) = secret_uri {
        return Sr25519Pair::from_uri(&secret_uri)
            .map_err(|e| serde::de::Error::custom(e.to_string()));
    }
    let mnemonic = Mnemonic::from_str(val);
    if let Ok(mnemonic) = mnemonic {
        return Sr25519Pair::from_phrase(&mnemonic, None)
            .map_err(|e| serde::de::Error::custom(e.to_string()));
    }

    Err(serde::de::Error::custom(format!(
        "Failed to parse {val} as SURI or mnemonic"
    )))
}
