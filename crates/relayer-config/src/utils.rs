use config::{Config, File};
use std::path::{Path, PathBuf};

use crate::{anchor::LinkedAnchorConfig, evm::Contract, substrate::Pallet};

use super::*;

/// A helper function that will search for all config files in the given directory and return them as a vec
/// of the paths.
///
/// Supported file extensions are:
/// - `.toml`.
/// - `.json`.
pub fn search_config_files<P: AsRef<Path>>(
    base_dir: P,
) -> webb_relayer_utils::Result<Vec<PathBuf>> {
    // A pattern that covers all toml or json files in the config directory and subdirectories.
    let toml_pattern = format!("{}/**/*.toml", base_dir.as_ref().display());
    let json_pattern = format!("{}/**/*.json", base_dir.as_ref().display());
    tracing::trace!(
        "Loading config files from {} and {}",
        toml_pattern,
        json_pattern
    );
    let toml_files = glob::glob(&toml_pattern)?;
    let json_files = glob::glob(&json_pattern)?;
    toml_files
        .chain(json_files)
        .map(|v| v.map_err(webb_relayer_utils::Error::from))
        .collect()
}

/// Try to parse the [`WebbRelayerConfig`] from the given config file(s).
pub fn parse_from_files(
    files: &[PathBuf],
) -> webb_relayer_utils::Result<WebbRelayerConfig> {
    let mut builder = Config::builder();
    let contracts: HashMap<String, Vec<Contract>> = HashMap::new();
    // read through all config files for the first time
    // build up a collection of [contracts]
    for config_file in files {
        tracing::trace!("Loading config file: {}", config_file.display());
        // get file extension
        let ext = config_file
            .extension()
            .map(|e| e.to_str().unwrap_or(""))
            .unwrap_or("");
        let format = match ext {
            "toml" => config::FileFormat::Toml,
            "json" => config::FileFormat::Json,
            _ => {
                tracing::warn!("Unknown file extension: {}", ext);
                continue;
            }
        };
        builder = builder
            .add_source(File::from(config_file.as_path()).format(format));
    }

    // also merge in the environment (with a prefix of WEBB).
    let builder = builder
        .add_source(config::Environment::with_prefix("WEBB").separator("_"));
    let cfg = builder.build()?;
    // and finally deserialize the config and post-process it
    let config: Result<
        WebbRelayerConfig,
        serde_path_to_error::Error<config::ConfigError>,
    > = serde_path_to_error::deserialize(cfg);
    match config {
        Ok(mut c) => {
            // merge in all of the contracts into the config
            for (network_name, network_chain) in c.evm.iter_mut() {
                if let Some(stored_contracts) = contracts.get(network_name) {
                    network_chain.contracts = stored_contracts.clone();
                }
            }

            postloading_process(c)
        }
        Err(e) => {
            tracing::error!("{}", e);
            Err(e.into())
        }
    }
}

/// Load the configuration files and
///
/// Returns `Ok(WebbRelayerConfig)` on success, or `Err(anyhow::Error)` on failure.
///
/// # Arguments
///
/// * `path` - The path to the configuration file
///
/// # Example
///
/// ```
/// use webb_relayer_config::utils::load;
///
/// let path = "/path/to/config.toml";
/// load(path);
/// ```
///
/// it is the same as using the [`search_config_files`] and [`parse_from_files`] functions combined.
pub fn load<P: AsRef<Path>>(
    path: P,
) -> webb_relayer_utils::Result<WebbRelayerConfig> {
    parse_from_files(&search_config_files(path)?)
}

/// The postloading_process exists to validate configuration and standardize
/// the format of the configuration
pub fn postloading_process(
    mut config: WebbRelayerConfig,
) -> webb_relayer_utils::Result<WebbRelayerConfig> {
    tracing::trace!("Checking configration sanity ...");

    // make all chain names lower case
    // 1. drain everything, and take enabled chains.
    let old_evm = config
        .evm
        .drain()
        .filter(|(_, chain)| chain.enabled)
        .collect::<HashMap<_, _>>();
    // 2. insert them again, as lowercased.
    for (_, v) in old_evm {
        config.evm.insert(v.chain_id.to_string(), v);
    }
    // do the same for substrate
    let old_substrate = config
        .substrate
        .drain()
        .filter(|(_, chain)| chain.enabled)
        .collect::<HashMap<_, _>>();
    for (_, v) in old_substrate {
        config.substrate.insert(v.chain_id.to_string(), v);
    }

    //Chain list is used to validate if linked anchor configuration is provided to the relayer.
    let mut chain_list: HashSet<webb_proposals::TypedChainId> = HashSet::new();
    // Convert linked anchor to Raw ResourceId type for evm chains
    for (_, network_chain) in config.evm.iter_mut() {
        let typed_chain_id =
            webb_proposals::TypedChainId::Evm(network_chain.chain_id);
        chain_list.insert(typed_chain_id);
        network_chain.contracts.iter_mut().for_each(|c| {
            if let Contract::VAnchor(cfg) = c {
                let linked_anchors = cfg.linked_anchors.clone();
                if let Some(linked_anchors) = linked_anchors {
                    let linked_anchors: Vec<LinkedAnchorConfig> =
                        linked_anchors
                            .into_iter()
                            .map(LinkedAnchorConfig::into_raw_resource_id)
                            .collect();
                    cfg.linked_anchors = Some(linked_anchors);
                }
            }
        })
    }
    // Convert linked anchor to Raw ResourceId type for substrate chains
    for (_, network_chain) in config.substrate.iter_mut() {
        let typed_chain_id =
            webb_proposals::TypedChainId::Substrate(network_chain.chain_id);
        chain_list.insert(typed_chain_id);
        network_chain.pallets.iter_mut().for_each(|c| {
            if let Pallet::VAnchorBn254(cfg) = c {
                let linked_anchors = cfg.linked_anchors.clone();
                if let Some(linked_anchors) = linked_anchors {
                    let linked_anchors: Vec<LinkedAnchorConfig> =
                        linked_anchors
                            .into_iter()
                            .map(LinkedAnchorConfig::into_raw_resource_id)
                            .collect();
                    cfg.linked_anchors = Some(linked_anchors);
                }
            }
        })
    }
    // check that all required chains are already present in the config.
    for (chain_id, chain_config) in &config.evm {
        let vanchors = chain_config.contracts.iter().filter_map(|c| match c {
            Contract::VAnchor(cfg) => Some(cfg),
            _ => None,
        });
        // validation checks for vanchor
        for anchor in vanchors {
            // validate config for data querying
            if config.features.data_query {
                // check if events watcher is enabled
                if !anchor.events_watcher.enabled {
                    tracing::warn!(
                        "!!WARNING!!: In order to enable data querying,
                        event-watcher should also be enabled for ({})",
                        anchor.common.address
                    );
                }
                // check if data-query is enabled in evenst-watcher config
                if !anchor.events_watcher.enable_data_query {
                    tracing::warn!(
                        "!!WARNING!!: In order to enable data querying,
                        enable-data-query in events-watcher config should also be enabled for ({})",
                        anchor.common.address
                    );
                }
            }
            // validate config for governance relaying
            if config.features.governance_relay {
                // check if proposal signing backend is configured
                if anchor.proposal_signing_backend.is_none() {
                    tracing::warn!(
                        "!!WARNING!!: In order to enable governance relaying,
                        proposal-signing-backend should be configured for ({})",
                        anchor.common.address
                    );
                }
                // check if event watchers is enabled
                if !anchor.events_watcher.enabled {
                    tracing::warn!(
                        "!!WARNING!!: In order to enable governance relaying,
                        event-watcher should also be enabled for ({})",
                        anchor.common.address
                    );
                }
                // check if linked anchor is configured
                match &anchor.linked_anchors {
                    None => {
                        tracing::warn!(
                            "!!WARNING!!: In order to enable governance relaying,
                            linked-anchors should also be configured for ({})",
                            anchor.common.address
                        );
                    }
                    Some(linked_anchors) => {
                        if linked_anchors.is_empty() {
                            tracing::warn!(
                                "!!WARNING!!: In order to enable governance relaying,
                                linked-anchors cannot be empty.
                                Please congigure Linked anchors for ({})",
                                anchor.common.address
                            );
                        } else {
                            for linked_anchor in linked_anchors {
                                match linked_anchor {
                                    LinkedAnchorConfig::Raw(raw_resource) => {
                                        let bytes: [u8; 32] = raw_resource.resource_id.into();
                                        let resource_id = webb_proposals::ResourceId::from(bytes);
                                        let typed_chain_id = resource_id.typed_chain_id();
                                        if !chain_list.contains(&typed_chain_id)
                                        {
                                            tracing::warn!("!!WARNING!!: Type: Evm, chain with id {} is not defined in the config.
                                                which is required by the Anchor Contract ({}) defined on {} chain.
                                                Please, define it manually, to allow the relayer to work properly.",
                                                typed_chain_id.chain_id(),
                                                anchor.common.address,
                                                chain_id
                                            );
                                        }
                                   }
                                   _=> unreachable!("Convert all linked anchor to Raw ResourceId type")
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    tracing::trace!(
        "postloaded config: {}",
        serde_json::to_string_pretty(&config)?
    );

    Ok(config)
}
