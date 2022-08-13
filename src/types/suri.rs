use serde::Deserialize;
use webb::substrate::subxt::sp_core::sr25519::Pair as Sr25519Pair;
use webb::substrate::subxt::sp_core::Pair;

/// [`Substrate Uri`](https://polkadot.js.org/docs/keyring/start/suri/)
#[derive(Clone)]
pub struct Suri(Sr25519Pair);

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
                            "error while loading this env {}: {}",
                            var, e,
                        ))
                    })?;
                    let maybe_pair =
                        Sr25519Pair::from_string_with_seed(&val, None);
                    match maybe_pair {
                        Ok((pair, _)) => Ok(pair),
                        Err(e) => {
                            Err(serde::de::Error::custom(format!("{:?}", e)))
                        }
                    }
                } else if value.starts_with('>') {
                    todo!("Implement command execution to extract the private key")
                } else {
                    let maybe_pair =
                        Sr25519Pair::from_string_with_seed(value, None);
                    match maybe_pair {
                        Ok((pair, _)) => Ok(pair),
                        Err(e) => {
                            Err(serde::de::Error::custom(format!("{:?}", e)))
                        }
                    }
                }
            }
        }

        let secret = deserializer.deserialize_str(PrivateKeyVistor)?;
        Ok(Self(secret))
    }
}
