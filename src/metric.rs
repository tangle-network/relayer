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

use prometheus::core::{AtomicF64, GenericCounter, GenericGauge};
use prometheus::{register_counter, register_gauge, Encoder, TextEncoder};

/// A struct definition for collecting metrics in the relayer
#[derive(Debug, Clone)]
pub struct Metrics {
    /// Bridge watcher back off metric
    pub bridge_watcher_back_off_metric: GenericCounter<AtomicF64>,
    /// Total active Relayer metric
    pub total_active_relayer_metric: GenericCounter<AtomicF64>,
    /// Total transaction made Relayer metric
    pub total_transaction_made_metric: GenericCounter<AtomicF64>,
    /// Anchor update proposals proposed by relayer
    pub anchor_update_proposals_metric: GenericCounter<AtomicF64>,
    /// No of proposal signed by dkg/mocked
    pub proposals_signed_metric: GenericCounter<AtomicF64>,
    /// Proposals dequeued and executed through transaction queue
    pub proposals_processed_tx_queue_metric: GenericCounter<AtomicF64>,
    /// Proposals dequeued and executed through transaction queue
    pub proposals_processed_substrate_tx_queue_metric:
        GenericCounter<AtomicF64>,
    /// Proposals dequeued and executed through transaction queue
    pub proposals_processed_evm_tx_queue_metric: GenericCounter<AtomicF64>,
    /// Transaction queue backoff metric
    pub transaction_queue_back_off_metric: GenericCounter<AtomicF64>,
    /// Substrate Transaction queue backoff metric
    pub substrate_transaction_queue_back_off_metric: GenericCounter<AtomicF64>,
    /// Evm Transaction queue backoff metric
    pub evm_transaction_queue_back_off_metric: GenericCounter<AtomicF64>,
    /// Total fees earned metric
    pub total_fee_earned_metric: GenericCounter<AtomicF64>,
    /// Gas spent metric
    pub gas_spent_metric: GenericCounter<AtomicF64>,
    /// Total amount of data stored metric
    pub total_amount_of_data_stored_metric: GenericGauge<AtomicF64>,
}

impl Metrics {
    /// Instantiates the various metrics and their counters, also creates a registry for the counters and
    /// registers the counters
    pub fn new() -> Self {
        let bridge_watcher_back_off_counter = register_counter!(
            "bridge_watcher_back_off_metric",
            "specifies how many times the bridge watcher backed off"
        );

        let total_active_relayer_metric_counter = register_counter!(
            "total_active_relayer_metric",
            "The total number of active relayers",
        );

        let total_transaction_made_metric_counter = register_counter!(
            "total_transaction_made_metric",
            "The total number of transaction made",
        );

        let anchor_update_proposals_metric_counter = register_counter!(
            "anchor_update_proposals_metric",
            "The total number of anchor update proposal proposed by relayer",
        );

        let proposals_signed_metric_counter = register_counter!(
            "proposals_signed_metric",
            "The total number of proposal signed by dkg/mocked backend",
        );

        let proposals_processed_tx_queue_metric_counter = register_counter!(
            "proposals_processed_tx_queue_metric",
            "Total number of signed proposals processed by transaction queue",
        );

        let proposals_processed_substrate_tx_queue_metric_counter = register_counter!(
            "proposals_processed_substrate_tx_queue_metric",
            "Total number of signed proposals processed by substrate transaction queue",
        );

        let proposals_processed_evm_tx_queue_metric_counter = register_counter!(
            "proposals_processed_evm_tx_queue_metric",
            "Total number of signed proposals processed by evm transaction queue",
        );

        let transaction_queue_back_off_metric_counter = register_counter!(
            "transaction_queue_back_off_metric",
            "How many times the transaction queue backed off",
        );

        let substrate_transaction_queue_back_off_metric_counter = register_counter!(
            "substrate_transaction_queue_back_off_metric",
            "How many times the substrate transaction queue backed off",
        );

        let evm_transaction_queue_back_off_metric_counter = register_counter!(
            "evm_transaction_queue_back_off_metric",
            "How many times the evm transaction queue backed off",
        );

        let total_fee_earned_metric_counter = register_counter!(
            "total_fee_earned_metric",
            "The total number of fees earned",
        );

        let gas_spent_metric_counter = register_counter!(
            "gas_spent_metric",
            "The total number of gas spent"
        );

        let total_amount_of_data_stored_metric_counter = register_gauge!(
            "total_amount_of_data_stored_metric",
            "The Total number of data stored",
        );

        Self {
            bridge_watcher_back_off_metric: bridge_watcher_back_off_counter
                .unwrap(),
            total_active_relayer_metric: total_active_relayer_metric_counter
                .unwrap(),
            total_transaction_made_metric:
                total_transaction_made_metric_counter.unwrap(),
            anchor_update_proposals_metric:
                anchor_update_proposals_metric_counter.unwrap(),
            proposals_signed_metric: proposals_signed_metric_counter.unwrap(),
            proposals_processed_tx_queue_metric:
                proposals_processed_tx_queue_metric_counter.unwrap(),
            proposals_processed_substrate_tx_queue_metric:
                proposals_processed_substrate_tx_queue_metric_counter.unwrap(),
            proposals_processed_evm_tx_queue_metric:
                proposals_processed_evm_tx_queue_metric_counter.unwrap(),
            transaction_queue_back_off_metric:
                transaction_queue_back_off_metric_counter.unwrap(),
            substrate_transaction_queue_back_off_metric:
                substrate_transaction_queue_back_off_metric_counter.unwrap(),
            evm_transaction_queue_back_off_metric:
                evm_transaction_queue_back_off_metric_counter.unwrap(),
            total_fee_earned_metric: total_fee_earned_metric_counter.unwrap(),
            gas_spent_metric: gas_spent_metric_counter.unwrap(),
            total_amount_of_data_stored_metric:
                total_amount_of_data_stored_metric_counter.unwrap(),
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
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}
