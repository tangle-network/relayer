use serde::{Deserialize, Serialize};
use tiny_keccak::{Hasher, Keccak};

/// CWChainId represents a cosmwasm(cosmos-sdk) chain-id.
#[derive(Clone)]
pub struct CWChainId(u32);

impl std::fmt::Debug for CWChainId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("CWChainId").field(&self.0).finish()
    }
}

impl std::ops::Deref for CWChainId {
    type Target = u32;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Serialize for CWChainId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_newtype_struct("CWChainId", &self.0)
    }
}

impl<'de> Deserialize<'de> for CWChainId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct CWChainIdVistor;
        impl<'de> serde::de::Visitor<'de> for CWChainIdVistor {
            type Value = u32;

            fn expecting(
                &self,
                formatter: &mut std::fmt::Formatter,
            ) -> std::fmt::Result {
                formatter.write_str("string chain ID")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let str_value = if value.starts_with("0x") {
                    // hex value
                    return Err(serde::de::Error::custom(format!(
                        "got {} but expected a string chain ID ",
                        value
                    )));
                } else if value.starts_with('>') {
                    todo!("Implement command execution to extract the cosmwasm chain ID")
                } else if value.starts_with('$') {
                    // env
                    let var = value.strip_prefix('$').unwrap_or(value);
                    tracing::trace!("Reading {} from env", var);
                    std::env::var(var).map_err(|e| {
                        serde::de::Error::custom(format!(
                            "error while loading this env {}: {}",
                            var, e,
                        ))
                    })?
                } else {
                    value.to_string()
                };
                Ok(compute_cosmwasm_chain_id(&str_value))
            }
        }

        let chain_id = deserializer.deserialize_str(CWChainIdVistor)?;
        Ok(Self(chain_id))
    }
}

/// Computes the numeric "chain_id" from string one.
/// This is only needed for Cosmos SDK blockchains since
/// their "chain_id"s are string(eg: "juno-1")
/// Rule:
///   1. Hash the "chain_id" to get 32-length bytes array
///       eg: keccak256("juno-1") => 4c22bf61f15534242ee9dba16dceb4c976851b1788680fb5ee2a7b568a294d21
///   2. Slice the last 4 bytes & convert it to `u32` numeric value
///       eg: 8a294d21(hex) -> 2317962529(decimal)
pub fn compute_cosmwasm_chain_id(chain_id_str: &str) -> u32 {
    let mut keccak = Keccak::v256();
    keccak.update(chain_id_str.as_bytes());

    let mut output = [0u8; 32];
    keccak.finalize(&mut output);

    let last_4_bytes = &output[28..];

    let mut buf = [0u8; 4];
    buf[0..4].copy_from_slice(last_4_bytes);
    u32::from_be_bytes(buf)
}
