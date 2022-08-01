use bip39::{Language, Mnemonic as BipMnemonic};
use serde::Deserialize;

/// Mnemonic represents a mnemonic.
#[derive(Clone)]
pub struct Mnemonic(BipMnemonic);

impl std::fmt::Debug for Mnemonic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("mnemonic").finish()
    }
}

impl std::ops::Deref for Mnemonic {
    type Target = BipMnemonic;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de> Deserialize<'de> for Mnemonic {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct MnemonicVistor;
        impl<'de> serde::de::Visitor<'de> for MnemonicVistor {
            type Value = BipMnemonic;

            fn expecting(
                &self,
                formatter: &mut std::fmt::Formatter,
            ) -> std::fmt::Result {
                formatter.write_str("12 or 24 word mnemonic seed phrase")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                BipMnemonic::from_phrase(value, Language::English).map_err(
                    |_| {
                        serde::de::Error::custom(format!(
                            "Cannot get the mnemonic from string: {}",
                            value
                        ))
                    },
                )
            }
        }

        let mnemonic = deserializer.deserialize_str(MnemonicVistor)?;
        Ok(Self(mnemonic))
    }
}
