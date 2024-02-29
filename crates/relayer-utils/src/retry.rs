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
