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

use std::collections::HashMap;

use prometheus::core::{AtomicF64, GenericCounter, GenericGauge};
use prometheus::{register_counter, register_gauge, Encoder, TextEncoder};
use webb_proposals::ResourceId;

/// A struct for collecting metrics for particular resource.
#[derive(Debug, Clone)]
pub struct ResourceMetric {
    /// Total gas spent on Resource.
    pub total_gas_spent: GenericCounter<AtomicF64>,
    /// Total fees earned on Resource.
    pub total_fee_earned: GenericCounter<AtomicF64>,
    /// Account Balance
    pub account_balance: GenericGauge<AtomicF64>,
}

/// A struct definition for collecting metrics in the relayer.
#[derive(Debug, Clone)]
pub struct Metrics {
    /// Bridge watcher back off metric
    pub bridge_watcher_back_off: GenericCounter<AtomicF64>,
    /// Total active Relayer metric
    pub total_active_relayer: GenericCounter<AtomicF64>,
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
    pub resource_metric_map: HashMap<ResourceId, ResourceMetric>,
}

impl Metrics {
    /// Instantiates the various metrics and their counters, also creates a registry for the counters and
    /// registers the counters
    pub fn new() -> Self {
        let bridge_watcher_back_off_counter = register_counter!(
            "bridge_watcher_back_off",
            "specifies how many times the bridge watcher backed off"
        );

        let total_active_relayer_counter = register_counter!(
            "total_active_relayer",
            "The total number of active relayers",
        );

        let total_transaction_made_counter = register_counter!(
            "total_transaction_made",
            "The total number of transaction made",
        );

        let anchor_update_proposals_counter = register_counter!(
            "anchor_update_proposals",
            "The total number of anchor update proposal proposed by relayer",
        );

        let proposals_signed_counter = register_counter!(
            "proposals_signed",
            "The total number of proposal signed by dkg/mocked backend",
        );

        let proposals_processed_tx_queue_counter = register_counter!(
            "proposals_processed_tx_queue",
            "Total number of signed proposals processed by transaction queue",
        );

        let proposals_processed_substrate_tx_queue_counter = register_counter!(
            "proposals_processed_substrate_tx_queue",
            "Total number of signed proposals processed by substrate transaction queue",
        );

        let proposals_processed_evm_tx_queue_counter = register_counter!(
            "proposals_processed_evm_tx_queue",
            "Total number of signed proposals processed by evm transaction queue",
        );

        let transaction_queue_back_off_counter = register_counter!(
            "transaction_queue_back_off",
            "How many times the transaction queue backed off",
        );

        let substrate_transaction_queue_back_off_counter = register_counter!(
            "substrate_transaction_queue_back_off",
            "How many times the substrate transaction queue backed off",
        );

        let evm_transaction_queue_back_off_counter = register_counter!(
            "evm_transaction_queue_back_off",
            "How many times the evm transaction queue backed off",
        );

        let total_fee_earned_counter = register_counter!(
            "total_fee_earned",
            "The total number of fees earned",
        );

        let gas_spent_counter =
            register_counter!("gas_spent", "The total number of gas spent");

        let total_amount_of_data_stored_counter = register_gauge!(
            "total_amount_of_data_stored",
            "The Total number of data stored",
        );

        let resource_metric_map: HashMap<ResourceId, ResourceMetric> =
            HashMap::new();

        Self {
            bridge_watcher_back_off: bridge_watcher_back_off_counter.unwrap(),
            total_active_relayer: total_active_relayer_counter.unwrap(),
            total_transaction_made: total_transaction_made_counter.unwrap(),
            anchor_update_proposals: anchor_update_proposals_counter.unwrap(),
            proposals_signed: proposals_signed_counter.unwrap(),
            proposals_processed_tx_queue: proposals_processed_tx_queue_counter
                .unwrap(),
            proposals_processed_substrate_tx_queue:
                proposals_processed_substrate_tx_queue_counter.unwrap(),
            proposals_processed_evm_tx_queue:
                proposals_processed_evm_tx_queue_counter.unwrap(),
            transaction_queue_back_off: transaction_queue_back_off_counter
                .unwrap(),
            substrate_transaction_queue_back_off:
                substrate_transaction_queue_back_off_counter.unwrap(),
            evm_transaction_queue_back_off:
                evm_transaction_queue_back_off_counter.unwrap(),
            total_fee_earned: total_fee_earned_counter.unwrap(),
            gas_spent: gas_spent_counter.unwrap(),
            total_amount_of_data_stored: total_amount_of_data_stored_counter
                .unwrap(),
            resource_metric_map,
        }
    }

    /// Gathers the whole relayer metrics
    pub fn gather_metrics() -> String {
        let mut buffer = Vec::new();
        let encoder = TextEncoder::new();
        // Gather the metrics.
        let metric_families = prometheus::gather();
        // Encode them to send.
        encoder.encode(&metric_families, &mut buffer).unwrap();

        String::from_utf8(buffer.clone()).unwrap()
    }

    /// Registers new counters to track metric for individual resources.
    pub fn register_resource_id_counters(
        resource_id: ResourceId,
    ) -> ResourceMetric {
        let resource_hex = hex::encode(resource_id.to_bytes().as_ref());
        // Total gas fee spent on particular resource.
        let total_gas_spent_counter = register_counter!(
            format!("{}_total_gas_spent", resource_hex),
            format!(
                "The total number of gas spent on resource : {}",
                resource_hex
            )
        );
        // Total fee earned on particular resource.
        let total_fee_earned_counter = register_counter!(
            format!("{}_total_fees_earned", resource_hex),
            format!(
                "The total number of fees earned on resource : {}",
                resource_hex
            )
        );
        // Account Balance
        let account_balance_counter = register_gauge!(
            format!("{}_account_balance", resource_hex),
            format!("Total account balance : {}", resource_hex)
        );

        ResourceMetric {
            total_gas_spent: total_gas_spent_counter.unwrap(),
            total_fee_earned: total_fee_earned_counter.unwrap(),
            account_balance: account_balance_counter.unwrap(),
        }
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}
