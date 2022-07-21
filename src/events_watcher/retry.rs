use std::time::Duration;

use backoff::backoff::Backoff;

/// Contant is a backoff policy which always returns
/// a constant duration, until it exceeds the maximum retry count.
#[derive(Debug)]
pub struct ConstantWithMaxRetryCount {
    interval: Duration,
    max_retry_count: usize,
    count: usize,
}

impl ConstantWithMaxRetryCount {
    /// Creates a new Constant backoff with `interval` contant
    /// backoff and max retry count `max_retry_count`.
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
        if self.count < self.max_retry_count {
            self.count += 1;
            Some(self.interval)
        } else {
            None
        }
    }

    fn reset(&mut self) {
        self.count = 0;
    }
}
