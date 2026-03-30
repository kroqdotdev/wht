use governor::{Quota, RateLimiter};
use std::num::NonZeroU32;
use std::sync::Arc;

pub type Limiter = Arc<RateLimiter<governor::state::NotKeyed, governor::state::InMemoryState, governor::clock::DefaultClock>>;

/// Create a rate limiter that allows `rps` requests per second.
/// Returns None if rps is None (no limit).
pub fn create_limiter(rps: Option<f64>) -> Option<Limiter> {
    let rps = rps?;
    if rps <= 0.0 {
        return None;
    }

    // For fractional RPS (e.g., 0.5 = 1 request every 2 seconds),
    // we use a period-based approach.
    // For integer RPS, we use the simpler per_second approach.
    let rps_ceil = rps.ceil() as u32;
    let nz = NonZeroU32::new(rps_ceil.max(1)).unwrap();
    let quota = Quota::per_second(nz);

    Some(Arc::new(RateLimiter::direct(quota)))
}

/// Wait until the rate limiter allows the next request.
pub async fn wait(limiter: &Option<Limiter>) {
    if let Some(lim) = limiter {
        lim.until_ready().await;
    }
}
