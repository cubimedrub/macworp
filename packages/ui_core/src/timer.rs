//! Timing helper that works in the browser and on desktop.

use std::time::Duration;

/// Sleeps without blocking, in the browser and on desktop.
///
/// # Arguments
/// * `duration` - How long to wait
pub(crate) async fn sleep(duration: Duration) {
    #[cfg(target_arch = "wasm32")]
    gloo_timers::future::sleep(duration).await;
    #[cfg(not(target_arch = "wasm32"))]
    tokio::time::sleep(duration).await;
}
