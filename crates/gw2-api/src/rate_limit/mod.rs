//! Client-side model of the GW2 API rate limit.
//!
//! The limit is enforced **per IP address**, not per API key: burst 300, refill
//! 5 tokens/second. Every client in the process — the public one and every
//! authenticated one — draws from the same budget, so by default they all share
//! [`global()`] instead of each owning a bucket.
//!
//! The budget is also shared with tools we don't control (other GW2 apps on the
//! machine, other devices behind the same IP), and the API sends no rate-limit
//! headers, so this is a *model* of the remaining budget, never a measurement.
//! [`RateLimiter::penalize`] exists for that reason: when the server answers
//! 429, the model was wrong, and zeroing it makes every caller back off together.
//!
//! Source: <https://wiki.guildwars2.com/wiki/API:Best_practices>

use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

/// GW2 API burst size — the number of requests that can be sent back-to-back.
pub const GW2_BURST_SIZE: u32 = 300;

/// GW2 API refill rate in tokens per second.
pub const GW2_REFILL_RATE_PER_SECOND: u32 = 5;

struct Bucket {
    remaining: f64,
    last_update: Instant,
}

/// A token bucket shared by every client that holds a clone of the same `Arc`.
pub struct RateLimiter {
    capacity: f64,
    refill_per_sec: f64,
    state: Mutex<Bucket>,
}

impl RateLimiter {
    /// Build a limiter with an explicit budget.
    ///
    /// Use this to hand an app a *partition* of the real budget — e.g. a Stream
    /// Deck plugin taking 60/min so a companion app can have the rest. Nothing
    /// enforces that partitions add up; that is the point of a partition.
    pub fn new(burst: u32, refill_per_second: u32) -> Self {
        Self::with_rates(f64::from(burst.max(1)), f64::from(refill_per_second.max(1)))
    }

    fn with_rates(capacity: f64, refill_per_sec: f64) -> Self {
        Self {
            capacity,
            refill_per_sec,
            state: Mutex::new(Bucket {
                remaining: capacity,
                last_update: Instant::now(),
            }),
        }
    }

    /// The full documented budget: burst 300, refill 5/s.
    pub fn gw2_default() -> Self {
        Self::new(GW2_BURST_SIZE, GW2_REFILL_RATE_PER_SECOND)
    }

    /// A fraction of the full budget, for apps that share the machine with other
    /// consumers. `fraction` is clamped to `(0.0, 1.0]`.
    pub fn gw2_share(fraction: f64) -> Self {
        let f = fraction.clamp(f64::MIN_POSITIVE, 1.0);
        Self::with_rates(
            f64::from(GW2_BURST_SIZE) * f,
            f64::from(GW2_REFILL_RATE_PER_SECOND) * f,
        )
    }

    /// Wait until a token is available, then consume it.
    pub async fn acquire(&self) {
        loop {
            let wait = {
                // Scoped so the guard is dropped before any await.
                let mut b = self.state.lock().unwrap_or_else(|e| e.into_inner());
                let elapsed = b.last_update.elapsed().as_secs_f64();
                b.last_update = Instant::now();
                b.remaining = (b.remaining + elapsed * self.refill_per_sec).min(self.capacity);

                if b.remaining >= 1.0 {
                    b.remaining -= 1.0;
                    return;
                }
                // Sleep exactly as long as the missing fraction of a token needs.
                (1.0 - b.remaining) / self.refill_per_sec
            };
            tokio::time::sleep(Duration::from_secs_f64(wait.max(0.001))).await;
        }
    }

    /// Zero the bucket after the server answered 429.
    ///
    /// The model can be wrong in only one direction that matters — thinking we
    /// have budget we don't — and a 429 is the only signal that ever reveals it.
    pub fn penalize(&self) {
        let mut b = self.state.lock().unwrap_or_else(|e| e.into_inner());
        b.remaining = 0.0;
        b.last_update = Instant::now();
    }

    /// Current modelled token count. Diagnostics only — never authoritative.
    pub fn modelled_remaining(&self) -> f64 {
        let b = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let elapsed = b.last_update.elapsed().as_secs_f64();
        (b.remaining + elapsed * self.refill_per_sec).min(self.capacity)
    }
}

static GLOBAL: LazyLock<Arc<RateLimiter>> = LazyLock::new(|| Arc::new(RateLimiter::gw2_default()));

/// The process-wide limiter every client uses unless given another one.
pub fn global() -> Arc<RateLimiter> {
    GLOBAL.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn burst_is_available_immediately() {
        let limiter = RateLimiter::new(3, 1);
        let start = Instant::now();
        for _ in 0..3 {
            limiter.acquire().await;
        }
        assert!(start.elapsed() < Duration::from_millis(50));
    }

    #[tokio::test]
    async fn penalize_empties_the_bucket() {
        let limiter = RateLimiter::new(300, 5);
        limiter.penalize();
        assert!(limiter.modelled_remaining() < 1.0);
    }

    #[test]
    fn share_scales_burst_and_refill() {
        let limiter = RateLimiter::gw2_share(0.2);
        assert!((limiter.capacity - 60.0).abs() < 1e-9);
        assert!((limiter.refill_per_sec - 1.0).abs() < 1e-9);
    }
}
