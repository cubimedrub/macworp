use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
    time::{Duration, Instant},
};

use macworp_core::workflow_configuration::engine::Engine;

use crate::configuration::Configuration;

/// What a worker announced and when
struct Entry {
    engines: Vec<Engine>,
    last_seen: Instant,
}

/// Workers that announced themselves, kept in memory: after a backend restart the workers
/// announce again within their interval. Assumes a single backend instance.
#[derive(Default)]
pub struct WorkerRegistry {
    workers: Mutex<HashMap<String, Entry>>,
}

impl WorkerRegistry {
    /// How long an announcement stays valid (the lease time, so a worker that lost its lease is
    /// also gone from the registry).
    pub fn ttl(&self, config: &Configuration) -> Duration {
        Duration::from_secs(u64::from(config.worker_lease_seconds()))
    }

    /// Records (or refreshes) the announcement of `worker`.
    ///
    /// # Arguments
    /// * `worker` - identity of the worker process, `username/worker_name`
    /// * `engines` - engines it can run
    /// * `now` - time of the announcement
    pub fn announce(&self, worker: &str, engines: Vec<Engine>, now: Instant) {
        let mut workers = self.workers.lock().expect("worker registry poisoned");
        workers.insert(
            worker.to_string(),
            Entry {
                engines,
                last_seen: now,
            },
        );
    }

    /// Engines of all workers announced less than `ttl` before `now`; also forgets the others.
    pub fn live_engines(&self, now: Instant, ttl: Duration) -> HashSet<Engine> {
        let mut workers = self.workers.lock().expect("worker registry poisoned");
        workers.retain(|_, entry| now.saturating_duration_since(entry.last_seen) < ttl);
        workers
            .values()
            .flat_map(|entry| entry.engines.iter().cloned())
            .collect()
    }

    /// Number of workers announced less than `ttl` before `now`.
    pub fn online(&self, now: Instant, ttl: Duration) -> usize {
        let workers = self.workers.lock().expect("worker registry poisoned");
        workers
            .values()
            .filter(|entry| now.saturating_duration_since(entry.last_seen) < ttl)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TTL: Duration = Duration::from_secs(30);

    #[test]
    fn union_of_live_workers() {
        let registry = WorkerRegistry::default();
        let start = Instant::now();
        registry.announce("w/a", vec![Engine::Nextflow], start);
        registry.announce("w/b", vec![Engine::Container], start);
        let engines = registry.live_engines(start, TTL);
        assert_eq!(engines, HashSet::from([Engine::Nextflow, Engine::Container]));
        assert_eq!(registry.online(start, TTL), 2);
    }

    #[test]
    fn announcements_expire_and_refresh() {
        let registry = WorkerRegistry::default();
        let start = Instant::now();
        registry.announce("w/a", vec![Engine::Nextflow], start);
        let later = start + TTL;
        assert_eq!(registry.online(later, TTL), 0);
        assert!(registry.live_engines(later, TTL).is_empty());

        registry.announce("w/a", vec![Engine::Snakemake], later);
        assert_eq!(
            registry.live_engines(later, TTL),
            HashSet::from([Engine::Snakemake])
        );
    }
}
