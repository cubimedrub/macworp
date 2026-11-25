//! What the desktop app lets its backend do with the container runtime of the machine it runs
//! on. The backend does not know how: the embedding app implements [`RuntimeControl`] with its
//! doctor and VM tooling, the `/runtime` routes only forward to it.

use std::{future::Future, pin::Pin};

use macworp_core::{runtime_status::{RuntimeStatus, SleepStatus}, workflow_configuration::engine::Engine};

/// Boxed future, so the control can be used as a trait object.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// The container runtime of the machine the backend runs on.
pub trait RuntimeControl: Send + Sync {
    /// The status as last checked, no new check.
    fn status(&self) -> BoxFuture<'_, RuntimeStatus>;

    /// Checks the runtime again and returns (and keeps) the new status.
    fn refresh(&self) -> BoxFuture<'_, RuntimeStatus>;

    /// Engines no usable runtime can run, as of the last check: runs of these are rejected.
    fn unavailable_engines(&self) -> Vec<Engine>;

    /// Starts the stopped container engine and returns the status once it answers.
    fn start(&self) -> BoxFuture<'_, Result<RuntimeStatus, String>>;

    /// Changes the memory of the container VM, restarting it, and returns the status
    /// afterwards. The caller makes sure no run is active.
    ///
    /// # Arguments
    /// * `mebibytes` - the new memory
    fn adjust_memory(&self, mebibytes: u64) -> BoxFuture<'_, Result<RuntimeStatus, String>>;

    /// Whether the computer is kept awake for the running workflows right now.
    fn sleep_status(&self) -> SleepStatus;
}
