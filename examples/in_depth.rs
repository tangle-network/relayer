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

//! In this example we will show how to use the webb relayer in depth.

#![deny(unsafe_code)]
#![allow(unused_variables)]

use std::collections::HashMap;
use std::sync::Arc;
use webb_relayer::service;
use webb_relayer_config::{
    event_watcher::EventsWatcherConfig,
    evm::{
        CommonContractConfig, Contract, EvmChainConfig,
        SignatureBridgeContractConfig, VAnchorContractConfig,
    },
    signing_backend::{
        MockedProposalSigningBackendConfig, ProposalSigningBackendConfig,
    },
    FeaturesConfig, WebbRelayerConfig,
};
use webb_relayer_context::RelayerContext;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // First things first we need to write our configrations.
    //
    // There is two options for this:
    // 1. Construct the configuration manually.
    // 2. Use the `config::load` function to load the configuration from a set of files.

    // Option 1:
    // Construct the configuration manually.
    let config = WebbRelayerConfig {
        port: 9955,
        features: FeaturesConfig {
            data_query: true,
            private_tx_relay: true,
            governance_relay: true,
        },
        proposal_signing_backend: Some(ProposalSigningBackendConfig::Mocked(
            MockedProposalSigningBackendConfig {
                private_key: ethereum_types::Secret::random().into(),
            },
        )),
        evm: HashMap::from([(
            String::from("polygon"),
            EvmChainConfig {
                name: String::from("polygon"),
                enabled: true,
                http_endpoint: "https://polygon-rpc.com/"
                    .parse::<url::Url>()?
                    .into(),
                ws_endpoint: "wss://polygon-rpc.com/"
                    .parse::<url::Url>()?
                    .into(),
                explorer: Some("https://polygonscan.com".parse()?),
                chain_id: 137,
                private_key: Some(ethereum_types::Secret::random().into()),
                beneficiary: Some(ethereum_types::Address::random()), // Do not ever hardcode a private key in production!
                contracts: vec![
                    Contract::VAnchor(VAnchorContractConfig {
                        common: CommonContractConfig {
                            address: ethereum_types::Address::random(),
                            deployed_at: 69420,
                        },
                        events_watcher: EventsWatcherConfig {
                            enable_data_query: true,
                            enabled: true,
                            polling_interval: 3000,
                            max_blocks_per_step: 1000,
                            print_progress_interval: 60_000,
                            sync_blocks_from: None,
                        },
                        linked_anchors: None,
                        smart_anchor_updates: Default::default(),
                    }),
                    Contract::SignatureBridge(SignatureBridgeContractConfig {
                        common: CommonContractConfig {
                            address: ethereum_types::Address::random(),
                            deployed_at: 69420,
                        },
                        events_watcher: EventsWatcherConfig {
                            enable_data_query: true,
                            enabled: true,
                            polling_interval: 3000,
                            max_blocks_per_step: 1000,
                            print_progress_interval: 60_000,
                            sync_blocks_from: None,
                        },
                    }),
                ],
                block_poller: None,
                block_confirmations: 0,
                tx_queue: Default::default(),
                relayer_fee_config: Default::default(),
            },
        )]),
        ..Default::default()
    };
    // Option 2:
    // Using this:
    let config = webb_relayer_config::utils::load("path/to/config/directory")?;
    // or this:
    let config_files = webb_relayer_config::utils::search_config_files(
        "path/to/config/directory",
    )?;
    let config = webb_relayer_config::utils::parse_from_files(&config_files)?;

    // next is to build the store, or the storage backend:
    let store = webb_relayer_store::sled::SledStore::open("path/to/store")?;

    // finally, after loading the config files, we can build the relayer context.
    let ctx = RelayerContext::new(config, store.clone()).await?;
    // or temporary store:
    let store = webb_relayer_store::sled::SledStore::temporary()?;

    // it is now up to you to start the web interface/server for the relayer and the background
    // services.

    // Start the web server:
    service::build_web_services(ctx.clone()).await?;
    // and also the background services:
    // this does not block, will fire the services on background tasks.
    service::ignite(ctx, Arc::new(store)).await?;
    Ok(())
}
