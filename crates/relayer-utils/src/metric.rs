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

use std::collections::HashMap;

use prometheus::core::{AtomicF64, GenericCounter, GenericGauge};
use prometheus::labels;
use prometheus::opts;
use prometheus::{register_counter, register_gauge, Encoder, TextEncoder};
use webb_proposals::{ResourceId, TargetSystem, TypedChainId};

/// A struct for collecting metrics for particular resource.
#[derive(Debug, Clone)]
pub struct ResourceMetric {
    /// Total gas spent (in gwei) on Resource.
    pub total_gas_spent: GenericCounter<AtomicF64>,
    /// Total fees earned on Resource.
    pub total_fee_earned: GenericCounter<AtomicF64>,
}

/// A struct definition for collecting metrics in the relayer.
#[derive(Debug, Clone)]
pub struct Metrics {
    /// Bridge watcher back off metric
    pub bridge_watcher_back_off: GenericCounter<AtomicF64>,
    /// Total transaction made Relayer metric
    pub total_transaction_made: GenericCounter<AtomicF64>,
    /// Anchor update proposals proposed by relayer
    pub anchor_update_proposals: GenericCounter<AtomicF64>,
    /// No of proposal signed by dkg/mocked
    pub proposals_signed: GenericCounter<AtomicF64>,
    /// Proposals dequeued and executed through transaction queue
    pub proposals_processed_tx_queue: GenericCounter<AtomicF64>,
    /// Proposals dequeued and executed through transaction queue
    pub proposals_processed_substrate_tx_queue: GenericCounter<AtomicF64>,
    /// Proposals dequeued and executed through transaction queue
    pub proposals_processed_evm_tx_queue: GenericCounter<AtomicF64>,
    /// Transaction queue backoff metric
    pub transaction_queue_back_off: GenericCounter<AtomicF64>,
    /// Substrate Transaction queue backoff metric
    pub substrate_transaction_queue_back_off: GenericCounter<AtomicF64>,
    /// Evm Transaction queue backoff metric
    pub evm_transaction_queue_back_off: GenericCounter<AtomicF64>,
    /// Total fees earned metric
    pub total_fee_earned: GenericCounter<AtomicF64>,
    /// Gas spent metric
    pub gas_spent: GenericCounter<AtomicF64>,
    /// Total amount of data stored metric
    pub total_amount_of_data_stored: GenericGauge<AtomicF64>,
    /// Resource metric
    resource_metric_map: HashMap<ResourceId, ResourceMetric>,
    /// Metric for account balance (in gwei) on specific chain
    account_balance: HashMap<TypedChainId, GenericGauge<AtomicF64>>,
}

impl Metrics {
    /// Instantiates the various metrics and their counters, also creates a registry for the counters and
    /// registers the counters
    pub fn new() -> Result<Self, prometheus::Error> {
        let bridge_watcher_back_off = register_counter!(
            "bridge_watcher_back_off",
            "specifies how many times the bridge watcher backed off"
        )?;

        let total_transaction_made = register_counter!(
            "total_transaction_made",
            "The total number of transaction made",
        )?;

        let anchor_update_proposals = register_counter!(
            "anchor_update_proposals",
            "The total number of anchor update proposal proposed by relayer",
        )?;

        let proposals_signed = register_counter!(
            "proposals_signed",
            "The total number of proposal signed by dkg/mocked backend",
        )?;

        let proposals_processed_tx_queue = register_counter!(
            "proposals_processed_tx_queue",
            "Total number of signed proposals processed by transaction queue",
        )?;

        let proposals_processed_substrate_tx_queue = register_counter!(
            "proposals_processed_substrate_tx_queue",
            "Total number of signed proposals processed by substrate transaction queue",
        )?;

        let proposals_processed_evm_tx_queue = register_counter!(
            "proposals_processed_evm_tx_queue",
            "Total number of signed proposals processed by evm transaction queue",
        )?;

        let transaction_queue_back_off = register_counter!(
            "transaction_queue_back_off",
            "How many times the transaction queue backed off",
        )?;

        let substrate_transaction_queue_back_off = register_counter!(
            "substrate_transaction_queue_back_off",
            "How many times the substrate transaction queue backed off",
        )?;

        let evm_transaction_queue_back_off = register_counter!(
            "evm_transaction_queue_back_off",
            "How many times the evm transaction queue backed off",
        )?;

        let total_fee_earned = register_counter!(
            "total_fee_earned",
            "The total number of fees earned",
        )?;

        let gas_spent =
            register_counter!("gas_spent", "The total number of gas spent")?;

        let total_amount_of_data_stored = register_gauge!(
            "total_amount_of_data_stored",
            "The Total number of data stored",
        )?;

        Ok(Self {
            bridge_watcher_back_off,
            total_transaction_made,
            anchor_update_proposals,
            proposals_signed,
            proposals_processed_tx_queue,
            proposals_processed_substrate_tx_queue,
            proposals_processed_evm_tx_queue,
            transaction_queue_back_off,
            substrate_transaction_queue_back_off,
            evm_transaction_queue_back_off,
            total_fee_earned,
            gas_spent,
            total_amount_of_data_stored,
            resource_metric_map: Default::default(),
            account_balance: Default::default(),
        })
    }

    /// Gathers the whole relayer metrics
    pub fn gather_metrics() -> Result<String, GatherMetricsError> {
        let mut buffer = Vec::new();
        let encoder = TextEncoder::new();
        // Gather the metrics.
        let metric_families = prometheus::gather();
        // Encode them to send.
        encoder.encode(&metric_families, &mut buffer)?;

        Ok(String::from_utf8(buffer.clone())?)
    }

    // TODO: move this to webb-proposals
    fn chain_name(chain: TypedChainId) -> &'static str {
        match chain {
            TypedChainId::None => "None",
            TypedChainId::Evm(_) => "Evm",
            TypedChainId::Substrate(_) => "Substrate",
            TypedChainId::PolkadotParachain(_) => "PolkadotParachain",
            TypedChainId::KusamaParachain(_) => "KusamaParachain",
            TypedChainId::RococoParachain(_) => "RococoParachain",
            TypedChainId::Cosmos(_) => "Cosmos",
            TypedChainId::Solana(_) => "Solana",
            TypedChainId::Ink(_) => "Ink",
            _ => unimplemented!("Chain not supported"),
        }
    }

    pub fn resource_metric_entry(
        &mut self,
        resource_id: ResourceId,
    ) -> &mut ResourceMetric {
        self.resource_metric_map
            .entry(resource_id)
            .or_insert_with(|| {
                Metrics::register_resource_id_counters(resource_id)
            })
    }

    pub fn account_balance_entry(
        &mut self,
        chain: TypedChainId,
    ) -> &mut GenericGauge<AtomicF64> {
        self.account_balance.entry(chain).or_insert_with(|| {
            let chain_id = chain.underlying_chain_id().to_string();
            register_gauge!(opts!(
                "chain_account_balance",
                "Total account balance on chain",
                labels!(
                    "chain_type" => Self::chain_name(chain),
                    "chain_id" => &chain_id,
                )
            ))
            .expect("create gauge for account balance")
        })
    }

    /// Registers new counters to track metric for individual resources.
    fn register_resource_id_counters(
        resource_id: ResourceId,
    ) -> ResourceMetric {
        let chain_id = resource_id
            .typed_chain_id()
            .underlying_chain_id()
            .to_string();
        let (target_system_type, target_system_value) =
            match resource_id.target_system() {
                TargetSystem::ContractAddress(address) => {
                    ("contract", hex::encode(address))
                }
                TargetSystem::Substrate(system) => (
                    "tree_id",
                    format!(
                        "{}, pallet_index: {}",
                        system.tree_id, system.pallet_index
                    ),
                ),
                _ => unimplemented!("Target system not supported"),
            };
        let labels = labels!(
            "chain_type" => Self::chain_name(resource_id.typed_chain_id()),
            "chain_id" => &chain_id,
            "target_system_type" => target_system_type,
            "target_system_value" => &target_system_value
        );

        // Total gas fee spent on particular resource.
        let total_gas_spent = register_counter!(opts!(
            "resource_total_gas_spent",
            "Total number of gas spent on resource",
            labels
        ))
        .expect("create counter for gas spent");

        // Total fee earned on particular resource.
        let total_fee_earned = register_counter!(opts!(
            "resource_total_fees_earned",
            "Total number of fees earned on resource",
            labels
        ))
        .expect("create counter for fees earned");

        ResourceMetric {
            total_gas_spent,
            total_fee_earned,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum GatherMetricsError {
    #[error(transparent)]
    PrometheusError(#[from] prometheus::Error),
    #[error(transparent)]
    FromUtf8Error(#[from] std::string::FromUtf8Error),
}
