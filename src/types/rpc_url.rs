use serde::Deserialize;

#[derive(derive_more::Display, Clone)]
pub struct RpcUrl(url::Url);

impl std::fmt::Debug for RpcUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RpcUrl").finish()
    }
}

impl From<RpcUrl> for url::Url {
    fn from(rpc_url: RpcUrl) -> Self {
        rpc_url.0
    }
}

impl std::ops::Deref for RpcUrl {
    type Target = url::Url;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'de> Deserialize<'de> for RpcUrl {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct RpcUrlVistor;
        impl<'de> serde::de::Visitor<'de> for RpcUrlVistor {
            type Value = url::Url;

            fn expecting(
                &self,
                formatter: &mut std::fmt::Formatter,
            ) -> std::fmt::Result {
                formatter.write_str(
                    "rpc url string or an env var containing a rpc url string in it",
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
                    let maybe_rpc_url = url::Url::parse(&val);
                    match maybe_rpc_url {
                        Ok(url) => Ok(url),
                        Err(e) => {
                            Err(serde::de::Error::custom(format!("{:?}", e)))
                        }
                    }
                } else {
                    let maybe_rpc_url = url::Url::parse(&value);
                    match maybe_rpc_url {
                        Ok(url) => Ok(url),
                        Err(e) => {
                            Err(serde::de::Error::custom(format!("{:?}", e)))
                        }
                    }
                }
            }
        }

        let rpc_url = deserializer.deserialize_str(RpcUrlVistor)?;
        Ok(Self(rpc_url))
    }
}
