//! Keeps the computer awake while a run executes, but only on mains power.
//!
//! Sleep suspends the container engine and every workflow in it. While at least one run is held
//! ([`SleepPrevention::hold`]) and the computer is not on battery, an idle-sleep inhibitor is
//! held. On battery it is released at once (heat, battery drain, a laptop in a bag), and taken
//! again when the charger is back. Only idle sleep is inhibited: closing the lid still sleeps.

use std::{
    sync::{Arc, Mutex, MutexGuard},
    time::Duration,
};

use macworp_core::runtime_status::SleepStatus;
use tokio::task::JoinHandle;

/// How often the power source is read while a run is held.
const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// Where the computer gets its power from.
pub trait PowerSource: Send + Sync {
    /// The computer runs on battery now. `false` on mains power and on machines without a
    /// battery or when it cannot be read (staying awake is then the safe side for the run).
    fn on_battery(&self) -> bool;
}

/// Holds the OS request not to go to sleep, until the returned value is dropped.
pub trait SleepInhibitor: Send + Sync {
    /// Asks the system not to sleep through inactivity.
    ///
    /// # Returns
    /// The request; dropping it releases it. The error says why the system refused.
    fn inhibit(&self) -> Result<Box<dyn Send>, String>;
}

/// [`PowerSource`] reading the batteries of the machine.
pub struct SystemPowerSource;

impl PowerSource for SystemPowerSource {
    fn on_battery(&self) -> bool {
        let Ok(manager) = starship_battery::Manager::new() else {
            return false;
        };
        let Ok(batteries) = manager.batteries() else {
            return false;
        };
        batteries
            .flatten()
            .any(|battery| battery.state() == starship_battery::State::Discharging)
    }
}

/// [`SleepInhibitor`] backed by the `keepawake` crate (IOKit, `SetThreadExecutionState`,
/// logind).
pub struct SystemSleepInhibitor;

impl SleepInhibitor for SystemSleepInhibitor {
    fn inhibit(&self) -> Result<Box<dyn Send>, String> {
        keepawake::Builder::default()
            .idle(true)
            .reason("A MAcWorP workflow is running")
            .app_name("MAcWorP")
            .app_reverse_domain("de.macworp")
            .create()
            .map(|awake| Box::new(awake) as Box<dyn Send>)
            .map_err(|error| error.to_string())
    }
}

/// What the holds share.
struct State {
    /// Runs currently held
    holds: usize,
    /// The inhibitor request while one is taken
    inhibited: Option<Box<dyn Send>>,
    /// The system refused the last request
    refused: bool,
    /// Last power reading
    on_battery: bool,
}

/// Decides when the computer is kept awake and tells what it does. Shared by all slots of a
/// worker; one per process.
pub struct SleepPrevention {
    power: Arc<dyn PowerSource>,
    inhibitor: Arc<dyn SleepInhibitor>,
    interval: Duration,
    state: Mutex<State>,
}

impl SleepPrevention {
    /// Prevention with the real power source and inhibitor.
    pub fn system() -> Arc<Self> {
        Self::new(
            Arc::new(SystemPowerSource),
            Arc::new(SystemSleepInhibitor),
            POLL_INTERVAL,
        )
    }

    /// Prevention with its own power source and inhibitor, for tests.
    ///
    /// # Arguments
    /// * `power` - tells whether the computer runs on battery
    /// * `inhibitor` - takes the request not to sleep
    /// * `interval` - wait between power readings while a run is held
    pub fn new(
        power: Arc<dyn PowerSource>,
        inhibitor: Arc<dyn SleepInhibitor>,
        interval: Duration,
    ) -> Arc<Self> {
        Arc::new(Self {
            power,
            inhibitor,
            interval,
            state: Mutex::new(State {
                holds: 0,
                inhibited: None,
                refused: false,
                on_battery: false,
            }),
        })
    }

    /// A run starts: keeps the computer awake while it is on mains power, until the returned
    /// hold is dropped. Must be called inside a tokio runtime.
    pub fn hold(self: &Arc<Self>) -> SleepHold {
        self.lock().holds += 1;
        self.evaluate();
        let prevention = Arc::clone(self);
        let watcher = tokio::spawn(async move {
            loop {
                tokio::time::sleep(prevention.interval).await;
                // Reading the batteries talks to the OS, keep it off the async threads
                let source = Arc::clone(&prevention.power);
                let on_battery = tokio::task::spawn_blocking(move || source.on_battery())
                    .await
                    .unwrap_or(false);
                prevention.lock().on_battery = on_battery;
                prevention.apply();
            }
        });
        SleepHold {
            prevention: Arc::clone(self),
            watcher,
        }
    }

    /// What the computer does now, for the UI.
    pub fn status(&self) -> SleepStatus {
        let state = self.lock();
        if state.holds == 0 {
            SleepStatus::Idle
        } else if state.inhibited.is_some() {
            SleepStatus::KeepingAwake
        } else if state.refused {
            SleepStatus::Unavailable
        } else {
            SleepStatus::OnBattery
        }
    }

    /// Reads the power source now and applies it.
    fn evaluate(&self) {
        let on_battery = self.power.on_battery();
        self.lock().on_battery = on_battery;
        self.apply();
    }

    /// Takes or releases the inhibitor to match the holds and the last power reading.
    fn apply(&self) {
        let mut state = self.lock();
        let wanted = state.holds > 0 && !state.on_battery;
        if wanted && state.inhibited.is_none() {
            match self.inhibitor.inhibit() {
                Ok(request) => {
                    state.inhibited = Some(request);
                    state.refused = false;
                    tracing::info!("Keeping the computer awake while workflows run");
                }
                Err(error) => {
                    if !state.refused {
                        tracing::warn!("Could not keep the computer awake: {error}");
                    }
                    state.refused = true;
                }
            }
        } else if !wanted {
            state.refused = false;
            if state.inhibited.take().is_some() {
                tracing::info!("No longer keeping the computer awake");
            }
        }
    }

    /// A hold ended.
    fn release(&self) {
        {
            let mut state = self.lock();
            state.holds = state.holds.saturating_sub(1);
        }
        self.apply();
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// A run that is protected from sleep. Dropping it (run ended, cancelled, task aborted) ends
/// the protection.
pub struct SleepHold {
    prevention: Arc<SleepPrevention>,
    watcher: JoinHandle<()>,
}

impl Drop for SleepHold {
    fn drop(&mut self) {
        self.watcher.abort();
        self.prevention.release();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::*;

    /// Power source the test switches.
    struct FakePower(AtomicBool);

    impl PowerSource for FakePower {
        fn on_battery(&self) -> bool {
            self.0.load(Ordering::SeqCst)
        }
    }

    /// Inhibitor counting its live requests; can refuse.
    struct FakeInhibitor {
        live: Arc<AtomicUsize>,
        refuse: AtomicBool,
    }

    /// A live request of [`FakeInhibitor`], released on drop.
    struct Request(Arc<AtomicUsize>);

    impl Drop for Request {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }

    impl SleepInhibitor for FakeInhibitor {
        fn inhibit(&self) -> Result<Box<dyn Send>, String> {
            if self.refuse.load(Ordering::SeqCst) {
                return Err("refused".to_string());
            }
            self.live.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(Request(Arc::clone(&self.live))))
        }
    }

    /// Prevention with fakes: the power switch, the live request counter and the inhibitor.
    fn setup(
        on_battery: bool,
    ) -> (
        Arc<SleepPrevention>,
        Arc<FakePower>,
        Arc<AtomicUsize>,
        Arc<FakeInhibitor>,
    ) {
        let power = Arc::new(FakePower(AtomicBool::new(on_battery)));
        let live = Arc::new(AtomicUsize::new(0));
        let inhibitor = Arc::new(FakeInhibitor {
            live: Arc::clone(&live),
            refuse: AtomicBool::new(false),
        });
        let prevention =
            SleepPrevention::new(power.clone(), inhibitor.clone(), Duration::from_millis(10));
        (prevention, power, live, inhibitor)
    }

    /// Waits until `condition` holds, fails after two seconds.
    async fn eventually(condition: impl Fn() -> bool) {
        for _ in 0..200 {
            if condition() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("condition not reached");
    }

    /// Idle until a run is held; awake on mains; released when the run ends.
    #[tokio::test]
    async fn holds_on_mains_and_releases_at_the_end() {
        let (prevention, _power, live, _inhibitor) = setup(false);
        assert_eq!(prevention.status(), SleepStatus::Idle);
        let hold = prevention.hold();
        assert_eq!(prevention.status(), SleepStatus::KeepingAwake);
        assert_eq!(live.load(Ordering::SeqCst), 1);
        drop(hold);
        assert_eq!(prevention.status(), SleepStatus::Idle);
        assert_eq!(live.load(Ordering::SeqCst), 0);
    }

    /// On battery nothing is inhibited, the status still warns.
    #[tokio::test]
    async fn battery_is_never_inhibited() {
        let (prevention, _power, live, _inhibitor) = setup(true);
        let _hold = prevention.hold();
        assert_eq!(prevention.status(), SleepStatus::OnBattery);
        assert_eq!(live.load(Ordering::SeqCst), 0);
    }

    /// Unplugging mid-run releases, plugging in again takes it back.
    #[tokio::test]
    async fn unplug_releases_and_replug_takes_again() {
        let (prevention, power, live, _inhibitor) = setup(false);
        let _hold = prevention.hold();
        power.0.store(true, Ordering::SeqCst);
        eventually(|| live.load(Ordering::SeqCst) == 0).await;
        assert_eq!(prevention.status(), SleepStatus::OnBattery);
        power.0.store(false, Ordering::SeqCst);
        eventually(|| live.load(Ordering::SeqCst) == 1).await;
        assert_eq!(prevention.status(), SleepStatus::KeepingAwake);
    }

    /// Several runs share one request; it ends with the last.
    #[tokio::test]
    async fn runs_share_one_request() {
        let (prevention, _power, live, _inhibitor) = setup(false);
        let first = prevention.hold();
        let second = prevention.hold();
        assert_eq!(live.load(Ordering::SeqCst), 1);
        drop(first);
        assert_eq!(live.load(Ordering::SeqCst), 1);
        drop(second);
        assert_eq!(live.load(Ordering::SeqCst), 0);
    }

    /// A refusing system shows as unavailable, not as battery.
    #[tokio::test]
    async fn refusal_is_reported() {
        let (prevention, _power, live, inhibitor) = setup(false);
        inhibitor.refuse.store(true, Ordering::SeqCst);
        let _hold = prevention.hold();
        assert_eq!(prevention.status(), SleepStatus::Unavailable);
        assert_eq!(live.load(Ordering::SeqCst), 0);
    }
}
