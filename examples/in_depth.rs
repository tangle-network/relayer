// Copyright 2022 Webb Technologies Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//

//! In this example we will show how to use the webb relayer in depth.

#![deny(unsafe_code)]
#![allow(unused_variables)]

use std::collections::HashMap;
use std::sync::Arc;
use webb_relayer::context::RelayerContext;
use webb_relayer::service;
use webb_relayer_config::{
    anchor::VAnchorWithdrawConfig,
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
                        },
                        withdraw_config: Some(VAnchorWithdrawConfig {
                            withdraw_gaslimit: 21000.into(),
                            withdraw_fee_percentage: 0.01,
                        }),
                        proposal_signing_backend: Some(
                            ProposalSigningBackendConfig::Mocked(
                                MockedProposalSigningBackendConfig {
                                    private_key:
                                        ethereum_types::Secret::random().into(),
                                },
                            ),
                        ),
                        linked_anchors: None,
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
                        },
                    }),
                ],
                block_listener: None,
                tx_queue: Default::default(),
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

    // finally, after loading the config files, we can build the relayer context.
    let ctx = RelayerContext::new(config);

    // next is to build the store, or the storage backend:
    let store = webb_relayer_store::sled::SledStore::open("path/to/store")?;
    // or temporary store:
    let store = webb_relayer_store::sled::SledStore::temporary()?;

    // it is now up to you to start the web interface/server for the relayer and the background
    // services.

    // Start the web server:
    let (addr, web_services) =
        service::build_web_services(ctx.clone(), store.clone())?;
    // and also the background services:
    // this does not block, will fire the services on background tasks.
    service::ignite(&ctx, Arc::new(store)).await?;

    println!("Listening on {}", addr);
    web_services.await;
    Ok(())
}
