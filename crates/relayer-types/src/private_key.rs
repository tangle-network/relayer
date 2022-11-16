use std::str::FromStr;

use ethereum_types::Secret;
use serde::Deserialize;
use webb::evm::ethers::signers::{coins_bip39::English, MnemonicBuilder};

/// PrivateKey represents a private key.
#[derive(Clone)]
pub struct PrivateKey(Secret);

impl std::fmt::Debug for PrivateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PrivateKey").finish()
    }
}

impl From<Secret> for PrivateKey {
    fn from(secret: Secret) -> Self {
        PrivateKey(secret)
    }
}

impl std::ops::Deref for PrivateKey {
    type Target = Secret;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de> Deserialize<'de> for PrivateKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct PrivateKeyVistor;
        impl<'de> serde::de::Visitor<'de> for PrivateKeyVistor {
            type Value = Secret;

            fn expecting(
                &self,
                formatter: &mut std::fmt::Formatter,
            ) -> std::fmt::Result {
                formatter.write_str(
                    "hex string or an env var containing a hex string in it",
                )
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if value.starts_with("0x") {
                    // hex value
                    let maybe_hex = Secret::from_str(value);
                    match maybe_hex {
                        Ok(val) => Ok(val),
                        Err(e) => Err(serde::de::Error::custom(format!("{e}\n got {} but expected a 66 string (including the 0x prefix)", value.len()))),                    }
                } else if value.starts_with('$') {
                    // env
                    let var = value.strip_prefix('$').unwrap_or(value);
                    tracing::trace!("Reading {} from env", var);
                    let val = std::env::var(var).map_err(|e| {
                        serde::de::Error::custom(format!(
                            "error while loading this env {}: {}",
                            var, e,
                        ))
                    })?;
                    let maybe_hex = Secret::from_str(&val);
                    match maybe_hex {
                        Ok(val) => Ok(val),
                        Err(e) => Err(serde::de::Error::custom(format!("{e}\n expected a 66 chars string (including the 0x prefix) but found {} char",  val.len())))
                    }
                } else if value.starts_with("file:") {
                    // Read secrets from the file path
                    let file_path =
                        value.strip_prefix("file:").unwrap_or(value);
                    let val =
                        std::fs::read_to_string(file_path).map_err(|e| {
                            serde::de::Error::custom(format!(
                                "error while reading file path {} : {}",
                                file_path, e
                            ))
                        })?;
                    if val.starts_with("0x") {
                        let maybe_hex = Secret::from_str(&val);
                        match maybe_hex {
                            Ok(val) => Ok(val),
                            Err(e) => Err(serde::de::Error::custom(format!("{e}\n expected a 66 chars string (including the 0x prefix) but found {} char",  val.len())))
                        }
                    } else {
                        // if secret file does not start with '0x' and has 12 or 24 words in it
                        let wallet = MnemonicBuilder::<English>::default()
                            .phrase(val.as_str())
                            .build()
                            .map_err(|e| {
                                serde::de::Error::custom(format!(
                                    "{e}\n expected valid mnemonic word list",
                                ))
                            })?;
                        let private_key: [u8; 32] =
                            wallet.signer().to_bytes().into();
                        Ok(Secret::from(&private_key))
                    }
                } else if value.starts_with('>') {
                    todo!("Implement command execution to extract the private key")
                } else {
                    // if it doesn't contains special characters and has 12 or 24 words in it
                    let wallet = MnemonicBuilder::<English>::default()
                        .phrase(value)
                        .build()
                        .map_err(|e| {
                            serde::de::Error::custom(format!(
                                "{e}\n expected valid mnemonic word list",
                            ))
                        })?;
                    let private_key: [u8; 32] =
                        wallet.signer().to_bytes().into();
                    Ok(Secret::from(&private_key))
                }
            }
        }

        let secret = deserializer.deserialize_str(PrivateKeyVistor)?;
        Ok(Self(secret))
    }
}
