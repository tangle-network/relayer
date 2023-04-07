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

//! Retry logic for async calls

use std::time::Duration;

use backoff::backoff::Backoff;

/// Constant with Max Retry Count is a backoff policy which always returns
/// a constant duration, until it exceeds the maximum retry count.
#[derive(Debug)]
pub struct ConstantWithMaxRetryCount {
    interval: Duration,
    max_retry_count: usize,
    count: usize,
}

impl ConstantWithMaxRetryCount {
    /// Creates a new Constant backoff with `interval` and `max_retry_count`.
    /// `interval` is the duration to wait between retries, and `max_retry_count` is the maximum
    /// number of retries, after which we return `None` to indicate that we should stop retrying.
    pub fn new(interval: Duration, max_retry_count: usize) -> Self {
        Self {
            interval,
            max_retry_count,
            count: 0,
        }
    }
}

impl Backoff for ConstantWithMaxRetryCount {
    fn next_backoff(&mut self) -> Option<Duration> {
        (self.count < self.max_retry_count).then(|| {
            self.count += 1;
            self.interval
        })
    }

    fn reset(&mut self) {
        self.count = 0;
    }
}
