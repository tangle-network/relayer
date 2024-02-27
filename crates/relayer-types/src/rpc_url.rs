// Copyright (C) 2022-2024 Webb Technologies Inc.
//
// Tangle is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// Tangle is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should receive a copy of the GNU General Public License
// If not, see <http://www.gnu.org/licenses/>.

use serde::{Deserialize, Serialize};

/// An RPC URL Wrapper around [`url::Url`] to support the `serde` deserialization
/// from environment variables.
#[derive(Clone, Serialize)]
pub struct RpcUrl(url::Url);

impl RpcUrl {
    /// Returns the inner [`url::Url`].
    pub fn as_url(&self) -> &url::Url {
        &self.0
    }
}

impl std::fmt::Display for RpcUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Display the inner url, not the wrapper.
        // with all the parts, scheme, host, port, path, query, fragment.
        let scheme = self.0.scheme();
        write!(f, "{scheme}")?;
        if let Some(host) = self.0.host_str() {
            write!(f, "://{host}")?;
        }
        if let Some(port) = self.0.port_or_known_default() {
            write!(f, ":{port}")?;
        }
        write!(f, "{}", self.0.path())?;

        if let Some(query) = self.0.query() {
            write!(f, "?{query}")?;
        }
        if let Some(fragment) = self.0.fragment() {
            write!(f, "#{fragment}")?;
        }
        Ok(())
    }
}

impl std::fmt::Debug for RpcUrl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self}")?;
        Ok(())
    }
}

impl From<RpcUrl> for url::Url {
    fn from(rpc_url: RpcUrl) -> Self {
        rpc_url.0
    }
}

impl From<url::Url> for RpcUrl {
    fn from(url: url::Url) -> Self {
        RpcUrl(url)
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
                            "error while loading this env {var}: {e}",
                        ))
                    })?;
                    let maybe_rpc_url = url::Url::parse(&val);
                    match maybe_rpc_url {
                        Ok(url) => Ok(url),
                        Err(e) => {
                            Err(serde::de::Error::custom(format!("{e:?}")))
                        }
                    }
                } else {
                    let maybe_rpc_url = url::Url::parse(value);
                    match maybe_rpc_url {
                        Ok(url) => Ok(url),
                        Err(e) => {
                            Err(serde::de::Error::custom(format!("{e:?}")))
                        }
                    }
                }
            }
        }

        let rpc_url = deserializer.deserialize_str(RpcUrlVistor)?;
        Ok(Self(rpc_url))
    }
}
