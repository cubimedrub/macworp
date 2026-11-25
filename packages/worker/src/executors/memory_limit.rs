use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

const MIB: u64 = 1024 * 1024;

/// Memory the engine container itself keeps (JVM of Nextflow, Toil, Snakemake) out of what the
/// runtime's VM has: the tasks get the rest.
const ENGINE_RESERVE_BYTES: u64 = 1024 * MIB;

/// Below this the VM is too small to give tasks a meaningful limit, the engine gets none.
const MIN_TASK_BYTES: u64 = 512 * MIB;

/// Memory in MiB the tasks of a run may use in total, shared by an executor and the code that
/// keeps it current. It is set from what the runtime says about its VM before each run, so a
/// change of the VM's memory applies to the next run without rebuilding the executors.
/// `0` stands for no limit.
#[derive(Debug, Clone, Default)]
pub struct MemoryLimit(Arc<AtomicU64>);

impl MemoryLimit {
    /// Sets the limit from the memory of the runtime's VM, see [`Self::tasks_mebibytes`].
    ///
    /// # Arguments
    /// * `vm_bytes` - memory the runtime can give to containers, `None` if unknown or the
    ///   runtime does not run in a VM (the host's memory is the limit then)
    pub fn set_from_vm(&self, vm_bytes: Option<u64>) {
        let mebibytes = vm_bytes.and_then(Self::tasks_mebibytes).unwrap_or(0);
        self.0.store(mebibytes, Ordering::Relaxed);
    }

    /// The limit in MiB, `None` for no limit.
    pub fn mebibytes(&self) -> Option<u64> {
        match self.0.load(Ordering::Relaxed) {
            0 => None,
            mebibytes => Some(mebibytes),
        }
    }

    /// What is left for tasks when the engine container takes its reserve.
    ///
    /// # Arguments
    /// * `vm_bytes` - memory the runtime's VM has
    pub fn tasks_mebibytes(vm_bytes: u64) -> Option<u64> {
        let tasks = vm_bytes.saturating_sub(ENGINE_RESERVE_BYTES);
        (tasks >= MIN_TASK_BYTES).then_some(tasks / MIB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * MIB;

    #[test]
    fn engine_reserve_is_kept_back() {
        assert_eq!(MemoryLimit::tasks_mebibytes(8 * GIB), Some(7 * 1024));
    }

    #[test]
    fn tiny_vm_gets_no_limit() {
        assert_eq!(MemoryLimit::tasks_mebibytes(GIB), None);
        assert_eq!(MemoryLimit::tasks_mebibytes(0), None);
    }

    #[test]
    fn handle_is_shared_between_clones_and_can_be_cleared() {
        let limit = MemoryLimit::default();
        let clone = limit.clone();
        assert_eq!(limit.mebibytes(), None);
        limit.set_from_vm(Some(4 * GIB));
        assert_eq!(clone.mebibytes(), Some(3 * 1024));
        limit.set_from_vm(None);
        assert_eq!(clone.mebibytes(), None);
    }
}
