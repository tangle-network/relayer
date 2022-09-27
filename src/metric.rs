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

use prometheus::{Opts, Registry, Counter};

/// A struct definition for collecting metrics in the relayer
#[derive(Debug, Clone)]
pub struct Metrics {
    /// Bridge watcher back off metric
    pub bridge_watcher_back_off_metric: Counter,
    /// Proposal handling execution
    pub handle_proposal_execution_metric: Counter,
    /// Transaction queue backoff metric
    pub transaction_queue_back_off_metric: Counter,
    /// Proposal queue attempt metric
    pub proposal_queue_attempt_metric: Counter,
    /// Total active Relayer metric
    pub total_active_relayer_metric: Counter,
    /// Total fees earned metric
    pub total_fee_earned_metric: Counter,
    /// Gas spent metric
    pub gas_spent_metric: Counter,
    /// Total number of proposals metric
    pub total_number_of_proposals_metric: Counter,
    /// Total number of data stroed metric
    pub total_number_of_data_stored_metric: Counter,
}

impl Metrics {
    /// Instantiates the various metrics and their counters, also creates a registry for the counters and
    /// registers the counters
    pub fn new() -> Self {
        // creates the counter for the metrics
        let  bridge_watcher_back_off_counter_opts = Opts::new("bridge_watcher_back_off_metric", "specifies how many times the bridge watcher backed off");
        let  bridge_watcher_back_off_counter = Counter::with_opts(bridge_watcher_back_off_counter_opts).unwrap();

        let handle_proposal_execution_metric_counter_opts = Opts::new("handle_proposal_execution_metric", "How many times did the function handle_proposal get executed");
        let handle_proposal_execution_metric_counter = Counter::with_opts(handle_proposal_execution_metric_counter_opts).unwrap();

        let transaction_queue_back_off_metric_counter_opts = Opts::new("transaction_queue_back_off_metric", "How many times the transaction queue backed off");
        let transaction_queue_back_off_metric_counter = Counter::with_opts(transaction_queue_back_off_metric_counter_opts).unwrap();

        let proposal_queue_attempt_metric_counter_opts = Opts::new("proposal_queue_attempt_metric", "How many times a proposal is attempted to be queued");
        let proposal_queue_attempt_metric_counter = Counter::with_opts(proposal_queue_attempt_metric_counter_opts).unwrap();

        let total_active_relayer_metric_counter_opts = Opts::new("total_active_relayer_metric", "The total number of active relayers");
        let total_active_relayer_metric_counter = Counter::with_opts(total_active_relayer_metric_counter_opts).unwrap();

        let total_fee_earned_metric_counter_opts = Opts::new("total_fee_earned_metric", "The total number of fees earned");
        let total_fee_earned_metric_counter = Counter::with_opts(total_fee_earned_metric_counter_opts).unwrap();

        let gas_spent_metric_counter_opts = Opts::new("gas_spent_metric", "The total number of gas spent");
        let gas_spent_metric_counter = Counter::with_opts(gas_spent_metric_counter_opts).unwrap();

        let total_number_of_proposals_metric_counter_opts = Opts::new("total_number_of_proposals_metric", "The total number of proposals proposed");
        let total_number_of_proposals_metric_counter = Counter::with_opts(total_number_of_proposals_metric_counter_opts).unwrap();

        let total_number_of_data_stored_metric_counter_opts = Opts::new("total_number_of_data_stored_metric", "The Total number of data stored");
        let total_number_of_data_stored_metric_counter = Counter::with_opts(total_number_of_data_stored_metric_counter_opts).unwrap();


        // creates a registry for the counters
        let registry = Registry::new();

        // registers the counters
        registry.register(Box::new(bridge_watcher_back_off_counter.clone())).unwrap();
        registry.register(Box::new(handle_proposal_execution_metric_counter.clone())).unwrap();
        registry.register(Box::new(transaction_queue_back_off_metric_counter.clone())).unwrap();
        registry.register(Box::new(proposal_queue_attempt_metric_counter.clone())).unwrap();
        registry.register(Box::new(total_active_relayer_metric_counter.clone())).unwrap();
        registry.register(Box::new(total_fee_earned_metric_counter.clone())).unwrap();
        registry.register(Box::new(gas_spent_metric_counter.clone())).unwrap();
        registry.register(Box::new(total_number_of_proposals_metric_counter.clone())).unwrap();
        registry.register(Box::new(total_number_of_data_stored_metric_counter.clone())).unwrap();

        Self {
            bridge_watcher_back_off_metric: bridge_watcher_back_off_counter,
            handle_proposal_execution_metric: handle_proposal_execution_metric_counter,
            transaction_queue_back_off_metric: transaction_queue_back_off_metric_counter,
            proposal_queue_attempt_metric: proposal_queue_attempt_metric_counter,
            total_active_relayer_metric: total_active_relayer_metric_counter,
            total_fee_earned_metric: total_fee_earned_metric_counter,
            gas_spent_metric: gas_spent_metric_counter,
            total_number_of_proposals_metric: total_number_of_proposals_metric_counter,
            total_number_of_data_stored_metric: total_number_of_data_stored_metric_counter
        }
    }
}
