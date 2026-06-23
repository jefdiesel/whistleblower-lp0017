//! A small clock abstraction so timestamps are injectable in tests.

/// Source of wall-clock time (Unix epoch).
pub trait Clock: Send + Sync {
    /// Current time in Unix milliseconds.
    fn now_ms(&self) -> u64;
    /// Current time in Unix nanoseconds (Waku message timestamps are ns).
    fn now_ns(&self) -> u64 {
        self.now_ms().saturating_mul(1_000_000)
    }
}

/// Real system clock.
#[derive(Clone, Copy, Default, Debug)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

/// A frozen clock for deterministic tests.
#[derive(Clone, Copy, Debug)]
pub struct FixedClock(pub u64);

impl Clock for FixedClock {
    fn now_ms(&self) -> u64 {
        self.0
    }
}
