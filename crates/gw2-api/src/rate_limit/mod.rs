//! Client-side rate limiting via [`governor`].
//!
//! Models the GW2 API's token-bucket rate limit:
//! - **Burst size**: 300 tokens — all 300 can be consumed instantly.
//! - **Refill rate**: 5 tokens/second (300/minute).
//!
//! Source: <https://wiki.guildwars2.com/wiki/API:Best_practices>

use std::num::NonZeroU32;

use governor::clock::DefaultClock;
use governor::middleware::NoOpMiddleware;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};

/// A rate limiter configured to match the GW2 API token bucket.
pub type GovRateLimiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock, NoOpMiddleware>;

/// GW2 API burst size — the initial number of tokens in the bucket.
///
/// Requests up to this count can be sent back-to-back with no delay.
/// Source: <https://wiki.guildwars2.com/wiki/API:Best_practices>
pub const GW2_BURST_SIZE: u32 = 300;

/// GW2 API refill rate in tokens per second.
///
/// After the burst is exhausted, the bucket refills at this rate.
/// Source: <https://wiki.guildwars2.com/wiki/API:Best_practices>
pub const GW2_REFILL_RATE_PER_SECOND: u32 = 5;

/// Build a rate limiter that matches the GW2 API's token-bucket behaviour.
///
/// `refill_per_second` sets the steady-state refill rate.
/// `burst` sets how many requests can be sent instantly before throttling begins.
///
/// At the GW2 defaults (5 RPS, burst 300) this allows up to 300 immediate
/// requests, then sustains 5 req/s indefinitely — matching the server exactly.
pub fn build_rate_limiter(refill_per_second: u32, burst: u32) -> GovRateLimiter {
    let rps = NonZeroU32::new(refill_per_second.max(1)).expect("refill rate must be non-zero");
    let burst = NonZeroU32::new(burst.max(1)).expect("burst must be non-zero");
    RateLimiter::direct(Quota::per_second(rps).allow_burst(burst))
}
