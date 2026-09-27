// api/limiter.rs
//
// The GW2 API rate limit is enforced **per IP address**, not per API key:
// max burst 300, refill 5 tokens/sec. That means every client in this process
// — the unauthenticated static-data client and every authenticated session —
// draws from one shared budget, so the bucket lives here rather than on
// `ApiClient`.
//
// It also means the budget is shared with tools we don't control (other GW2
// apps on the machine, other devices behind the same IP), and the API returns
// no rate-limit headers, so this is a *model* of the remaining budget and never
// a measurement. `penalize()` exists for that reason: when the server tells us
// we were wrong by answering 429, we zero the model so every caller in the
// process backs off together.

use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

/// GW2's documented bucket size (max burst).
pub const GW2_BURST: f64 = 300.0;
/// GW2's documented refill rate, in tokens per second.
pub const GW2_REFILL_PER_SEC: f64 = 5.0;

struct Bucket {
    remaining: f64,
    last_update: Instant,
}

/// A token bucket shared by every [`ApiClient`](crate::api::client::ApiClient)
/// that is handed a clone of the same `Arc`.
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
    /// enforces that the partitions add up; that is the point of a partition.
    pub fn new(capacity: f64, refill_per_sec: f64) -> Self {
        Self {
            capacity,
            refill_per_sec,
            state: Mutex::new(Bucket {
                remaining: capacity,
                last_update: Instant::now(),
            }),
        }
    }

    /// The full documented budget: burst 300, refill 5/sec.
    pub fn gw2_default() -> Self {
        Self::new(GW2_BURST, GW2_REFILL_PER_SEC)
    }

    /// A fraction of the full budget, for apps that share the machine with
    /// other consumers. `fraction` is clamped to `(0.0, 1.0]`.
    pub fn gw2_share(fraction: f64) -> Self {
        let f = fraction.clamp(f64::MIN_POSITIVE, 1.0);
        Self::new(GW2_BURST * f, GW2_REFILL_PER_SEC * f)
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

    /// Zero the bucket after the server told us we were over the limit.
    ///
    /// Our model can be wrong in only one direction that matters — thinking we
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

/// The process-wide limiter every client uses unless told otherwise.
pub fn global() -> Arc<RateLimiter> {
    GLOBAL.clone()
}
