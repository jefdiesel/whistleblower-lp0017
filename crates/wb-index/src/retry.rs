//! Exponential-backoff retry, used to satisfy LP-0017's reliability requirement:
//! "Upload retries on transient Logos Storage failures with exponential back-off
//! and surfaces a clear error after exhausting retries."

use std::future::Future;
use std::time::Duration;

/// Backoff configuration.
#[derive(Clone, Debug)]
pub struct RetryPolicy {
    /// Number of retries *after* the initial attempt (so total attempts = 1 + max_retries).
    pub max_retries: u32,
    /// Delay before the first retry.
    pub base_delay: Duration,
    /// Upper bound on any single delay.
    pub max_delay: Duration,
    /// Geometric growth factor between retries.
    pub multiplier: u32,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 5,
            base_delay: Duration::from_millis(200),
            max_delay: Duration::from_secs(10),
            multiplier: 2,
        }
    }
}

impl RetryPolicy {
    /// No retries — fail on first error. Handy for tests.
    pub fn none() -> Self {
        Self {
            max_retries: 0,
            ..Self::default()
        }
    }

    /// Delay before retry number `attempt` (1-based): `base * multiplier^(attempt-1)`, capped.
    pub fn delay_for(&self, attempt: u32) -> Duration {
        let factor = self.multiplier.saturating_pow(attempt.saturating_sub(1));
        self.base_delay.saturating_mul(factor).min(self.max_delay)
    }
}

/// Run `op` with exponential backoff. `is_transient` gates which errors are
/// retried; non-transient errors fail immediately. On exhaustion the last error
/// is returned alongside the total number of attempts made.
pub async fn retry_async<T, E, F, Fut>(
    policy: &RetryPolicy,
    is_transient: impl Fn(&E) -> bool,
    mut op: F,
) -> Result<T, (u32, E)>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let mut attempt: u32 = 0;
    loop {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                attempt += 1;
                if attempt > policy.max_retries || !is_transient(&e) {
                    return Err((attempt, e));
                }
                tracing::warn!(attempt, "operation failed, retrying after backoff");
                tokio::time::sleep(policy.delay_for(attempt)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn backoff_grows_geometrically_and_caps() {
        let p = RetryPolicy {
            max_retries: 10,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_millis(450),
            multiplier: 2,
        };
        assert_eq!(p.delay_for(1), Duration::from_millis(100));
        assert_eq!(p.delay_for(2), Duration::from_millis(200));
        assert_eq!(p.delay_for(3), Duration::from_millis(400));
        assert_eq!(p.delay_for(4), Duration::from_millis(450)); // capped
    }

    #[tokio::test(start_paused = true)]
    async fn retries_then_succeeds() {
        let calls = Cell::new(0);
        let res: Result<u32, (u32, ())> = retry_async(
            &RetryPolicy {
                max_retries: 5,
                base_delay: Duration::from_millis(1),
                ..RetryPolicy::default()
            },
            |_| true,
            || {
                let n = calls.get() + 1;
                calls.set(n);
                async move {
                    if n < 3 {
                        Err(())
                    } else {
                        Ok(n)
                    }
                }
            },
        )
        .await;
        assert_eq!(res, Ok(3));
        assert_eq!(calls.get(), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn exhausts_and_reports_attempts() {
        let res: Result<(), (u32, &str)> = retry_async(
            &RetryPolicy {
                max_retries: 2,
                base_delay: Duration::from_millis(1),
                ..RetryPolicy::default()
            },
            |_| true,
            || async { Err("boom") },
        )
        .await;
        // 1 initial + 2 retries = 3 attempts
        assert_eq!(res, Err((3, "boom")));
    }

    #[tokio::test(start_paused = true)]
    async fn non_transient_fails_immediately() {
        let calls = Cell::new(0);
        let res: Result<(), (u32, ())> = retry_async(
            &RetryPolicy::default(),
            |_| false, // nothing is transient
            || {
                calls.set(calls.get() + 1);
                async { Err(()) }
            },
        )
        .await;
        assert_eq!(res, Err((1, ())));
        assert_eq!(calls.get(), 1);
    }
}
