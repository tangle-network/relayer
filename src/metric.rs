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

use prometheus::core::{AtomicF64, GenericCounter};
use prometheus::{register_counter, Encoder, TextEncoder};

/// A struct definition for collecting metrics in the relayer
#[derive(Debug, Clone)]
pub struct Metrics {
    /// Bridge watcher back off metric
    pub bridge_watcher_back_off_metric: GenericCounter<AtomicF64>,
    /// Proposal handling execution
    pub handle_proposal_execution_metric: GenericCounter<AtomicF64>,
    /// Transaction queue backoff metric
    pub transaction_queue_back_off_metric: GenericCounter<AtomicF64>,
    /// Proposal queue attempt metric
    pub proposal_queue_attempt_metric: GenericCounter<AtomicF64>,
    /// Total active Relayer metric
    pub total_active_relayer_metric: GenericCounter<AtomicF64>,
    /// Total transaction made Relayer metric
    pub total_transaction_made_metric: GenericCounter<AtomicF64>,
    /// Total fees earned metric
    pub total_fee_earned_metric: GenericCounter<AtomicF64>,
    /// Gas spent metric
    pub gas_spent_metric: GenericCounter<AtomicF64>,
    /// Total number of proposals metric
    pub total_number_of_proposals_metric: GenericCounter<AtomicF64>,
    /// Total number of data stored metric
    pub total_number_of_data_stored_metric: GenericCounter<AtomicF64>,
}

impl Metrics {
    /// Instantiates the various metrics and their counters, also creates a registry for the counters and
    /// registers the counters
    pub fn new() -> Self {
        let bridge_watcher_back_off_counter = register_counter!(
            "bridge_watcher_back_off_metric",
            "specifies how many times the bridge watcher backed off"
        );

        let handle_proposal_execution_metric_counter = register_counter!(
            "handle_proposal_execution_metric",
            "How many times did the function handle_proposal get executed",
        );

        let transaction_queue_back_off_metric_counter = register_counter!(
            "transaction_queue_back_off_metric",
            "How many times the transaction queue backed off",
        );

        let proposal_queue_attempt_metric_counter = register_counter!(
            "proposal_queue_attempt_metric",
            "How many times a proposal is attempted to be queued",
        );

        let total_active_relayer_metric_counter = register_counter!(
            "total_active_relayer_metric",
            "The total number of active relayers",
        );

        let total_transaction_made_metric_counter = register_counter!(
            "total_transaction_made_metric",
            "The total number of transaction made",
        );

        let total_fee_earned_metric_counter = register_counter!(
            "total_fee_earned_metric",
            "The total number of fees earned",
        );

        let gas_spent_metric_counter = register_counter!(
            "gas_spent_metric",
            "The total number of gas spent"
        );

        let total_number_of_proposals_metric_counter = register_counter!(
            "total_number_of_proposals_metric",
            "The total number of proposals proposed",
        );

        let total_number_of_data_stored_metric_counter = register_counter!(
            "total_number_of_data_stored_metric",
            "The Total number of data stored",
        );

        Self {
            bridge_watcher_back_off_metric: bridge_watcher_back_off_counter
                .unwrap(),
            handle_proposal_execution_metric:
                handle_proposal_execution_metric_counter.unwrap(),
            transaction_queue_back_off_metric:
                transaction_queue_back_off_metric_counter.unwrap(),
            proposal_queue_attempt_metric:
                proposal_queue_attempt_metric_counter.unwrap(),
            total_active_relayer_metric: total_active_relayer_metric_counter
                .unwrap(),
            total_transaction_made_metric:
                total_transaction_made_metric_counter.unwrap(),
            total_fee_earned_metric: total_fee_earned_metric_counter.unwrap(),
            gas_spent_metric: gas_spent_metric_counter.unwrap(),
            total_number_of_proposals_metric:
                total_number_of_proposals_metric_counter.unwrap(),
            total_number_of_data_stored_metric:
                total_number_of_data_stored_metric_counter.unwrap(),
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
