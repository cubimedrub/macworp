use std::sync::Arc;

use macworp_core::workflow_configuration::engine::Engine;
use tokio::sync::{mpsc, watch};

use super::{
    Executor, Job, LogLine, MemoryLimit, Outcome,
    container::ContainerExecutor,
    cwl::{self, CwlExecutor},
    nextflow::{self, NextflowExecutor},
    snakemake::{self, SnakemakeExecutor},
};
use crate::container_runtime::{BoxFuture, ContainerRuntime, RuntimeKind};

/// An executor tagged with the runtime it was built for, so the worker knows how its
/// containers reach the host.
struct OnRuntime {
    /// The wrapped executor
    inner: Arc<dyn Executor>,
    /// Runtime `inner` starts its containers with
    kind: RuntimeKind,
    /// The runtime itself, asked for the memory of its VM before each run
    runtime: Arc<dyn ContainerRuntime>,
    /// Limit handle shared with `inner`, `None` for engines without a memory option
    memory: Option<MemoryLimit>,
}

/// Whether the runtime's containers run in a VM of limited memory on this host: Docker Desktop
/// and podman machine on macOS and Windows. On Linux containers share the host's memory, and
/// `wslc` reports none.
///
/// # Arguments
/// * `kind` - the runtime
fn runs_in_vm(kind: RuntimeKind) -> bool {
    cfg!(any(windows, target_os = "macos")) && kind != RuntimeKind::Wslc
}

impl OnRuntime {
    /// Sets the memory limit of `inner` from what the runtime reports now, so a VM that was
    /// resized since the worker started is respected. A runtime that does not answer leaves no
    /// limit.
    async fn refresh_memory_limit(&self) {
        let Some(memory) = &self.memory else {
            return;
        };
        if !runs_in_vm(self.kind) {
            return;
        }
        match self.runtime.info().await {
            Ok(info) => memory.set_from_vm(info.memory_bytes),
            Err(error) => {
                tracing::warn!("Cannot read the memory of {}: {error}", self.kind);
                memory.set_from_vm(None);
            }
        }
    }
}

impl Executor for OnRuntime {
    fn engine(&self) -> Engine {
        self.inner.engine()
    }

    fn runtime_kind(&self) -> Option<RuntimeKind> {
        Some(self.kind)
    }

    fn run<'a>(
        &'a self,
        job: &'a Job,
        logs: mpsc::Sender<LogLine>,
        cancel: watch::Receiver<bool>,
    ) -> BoxFuture<'a, Outcome> {
        Box::pin(async move {
            self.refresh_memory_limit().await;
            self.inner.run(job, logs, cancel).await
        })
    }
}

/// The executors of all engines the `runtimes` can run, in order of preference: each engine
/// runs on the first runtime that supports it. Engines no runtime supports are left out.
/// Executors start sibling containers through the socket of their runtime when it has one.
///
/// # Arguments
/// * `runtimes` - usable runtimes, most preferred first
pub async fn standard_executors(runtimes: &[Arc<dyn ContainerRuntime>]) -> Vec<Arc<dyn Executor>> {
    let mut executors: Vec<Arc<dyn Executor>> = Vec::new();
    for engine in [
        Engine::Nextflow,
        Engine::Snakemake,
        Engine::Cwl,
        Engine::Container,
    ] {
        let Some(runtime) = runtimes
            .iter()
            .find(|runtime| runtime.supports_engine(&engine))
        else {
            continue;
        };
        let binary = runtime.binary().to_path_buf();
        // Nextflow and Toil talk to the runtime socket, Snakemake runs its tool containers with Apptainer
        let uses_socket = matches!(engine, Engine::Nextflow | Engine::Cwl);
        let engine_access = if !uses_socket {
            Vec::new()
        } else {
            match runtime.info().await {
                Ok(info) => info.engine_access_args().unwrap_or_default(),
                Err(e) => {
                    tracing::warn!("Unable to query container runtime: {e}");
                    Vec::new()
                }
            }
        };
        if uses_socket && engine_access.is_empty() {
            tracing::warn!(
                "No {} socket found, {engine} cannot start sibling containers",
                runtime.kind()
            );
        }
        let mapping = runtime.kind().path_mapping();
        let memory = MemoryLimit::default();
        let inner: Arc<dyn Executor> = match engine {
            Engine::Nextflow => Arc::new(
                NextflowExecutor::new(&binary, nextflow::image())
                    .with_path_mapping(mapping)
                    .with_memory_limit(memory.clone())
                    .with_engine_access(engine_access),
            ),
            Engine::Cwl => Arc::new(
                CwlExecutor::new(&binary, cwl::image())
                    .with_path_mapping(mapping)
                    .with_runtime_kind(runtime.kind())
                    .with_memory_limit(memory.clone())
                    .with_engine_access(engine_access),
            ),
            Engine::Snakemake => Arc::new(
                SnakemakeExecutor::new(&binary, snakemake::image())
                    .with_path_mapping(mapping)
                    .with_runtime_kind(runtime.kind())
                    .with_memory_limit(memory.clone())
                    .with_apparmor_profile(snakemake::apparmor_profile()),
            ),
            _ => Arc::new(ContainerExecutor::new(&binary).with_path_mapping(mapping)),
        };
        executors.push(Arc::new(OnRuntime {
            inner,
            kind: runtime.kind(),
            runtime: runtime.clone(),
            memory: matches!(engine, Engine::Nextflow | Engine::Snakemake | Engine::Cwl)
                .then_some(memory),
        }));
    }
    executors
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::container_runtime::{RuntimeError, RuntimeInfo};

    /// Runtime stand-in that only knows its kind and binary name.
    #[derive(Debug)]
    struct Fake(RuntimeKind);

    impl ContainerRuntime for Fake {
        fn kind(&self) -> RuntimeKind {
            self.0
        }
        fn binary(&self) -> &Path {
            Path::new("fake")
        }
        fn info(&self) -> BoxFuture<'_, Result<RuntimeInfo, RuntimeError>> {
            let kind = self.0;
            Box::pin(async move {
                Ok(RuntimeInfo {
                    kind,
                    version: "1.0".into(),
                    rootless: Some(true),
                    storage_root: None,
                    socket: None,
                    socket_exists: None,
                    unsupported_socket: None,
                    memory_bytes: None,
                })
            })
        }
    }

    /// Runtime that reports a VM with 4 GiB.
    #[derive(Debug)]
    struct Vm;

    impl ContainerRuntime for Vm {
        fn kind(&self) -> RuntimeKind {
            RuntimeKind::Podman
        }
        fn binary(&self) -> &Path {
            Path::new("fake")
        }
        fn info(&self) -> BoxFuture<'_, Result<RuntimeInfo, RuntimeError>> {
            Box::pin(async {
                Ok(RuntimeInfo {
                    kind: RuntimeKind::Podman,
                    version: "1.0".into(),
                    rootless: Some(true),
                    storage_root: None,
                    socket: None,
                    socket_exists: None,
                    unsupported_socket: None,
                    memory_bytes: Some(4 * 1024 * 1024 * 1024),
                })
            })
        }
    }

    /// The limit follows what the runtime reports before a run, but only where containers run
    /// in a VM (macOS, Windows); on Linux the host's memory applies and no limit is set.
    #[tokio::test]
    async fn memory_limit_is_read_from_the_runtime_in_a_vm_only() {
        let memory = MemoryLimit::default();
        let on_runtime = OnRuntime {
            inner: Arc::new(ContainerExecutor::new("fake")),
            kind: RuntimeKind::Podman,
            runtime: Arc::new(Vm),
            memory: Some(memory.clone()),
        };
        on_runtime.refresh_memory_limit().await;
        let expected = cfg!(any(windows, target_os = "macos")).then_some(3 * 1024);
        assert_eq!(memory.mebibytes(), expected);
    }

    /// Runtime kind per engine of the executors built for `kinds`.
    async fn assignment(kinds: &[RuntimeKind]) -> Vec<(Engine, Option<RuntimeKind>)> {
        let runtimes: Vec<Arc<dyn ContainerRuntime>> = kinds
            .iter()
            .map(|kind| Arc::new(Fake(*kind)) as Arc<dyn ContainerRuntime>)
            .collect();
        standard_executors(&runtimes)
            .await
            .iter()
            .map(|executor| (executor.engine(), executor.runtime_kind()))
            .collect()
    }

    /// With podman and docker every engine takes the first runtime supporting it: Snakemake goes
    /// to docker unless podman supports it too (Linux hosts), the rest stays on podman.
    #[tokio::test]
    async fn engines_use_the_first_runtime_that_supports_them() {
        let snakemake_runtime = if RuntimeKind::podman_runs_snakemake() {
            RuntimeKind::Podman
        } else {
            RuntimeKind::Docker
        };
        assert_eq!(
            assignment(&[RuntimeKind::Podman, RuntimeKind::Docker]).await,
            vec![
                (Engine::Nextflow, Some(RuntimeKind::Podman)),
                (Engine::Snakemake, Some(snakemake_runtime)),
                (Engine::Cwl, Some(RuntimeKind::Podman)),
                (Engine::Container, Some(RuntimeKind::Podman)),
            ]
        );
    }

    /// Podman alone has no Snakemake except on Linux hosts, docker alone runs everything.
    #[tokio::test]
    async fn single_runtimes() {
        let podman = assignment(&[RuntimeKind::Podman]).await;
        if RuntimeKind::podman_runs_snakemake() {
            assert_eq!(podman.len(), 4);
        } else {
            assert_eq!(podman.len(), 3);
            assert!(
                podman
                    .iter()
                    .all(|(engine, _)| *engine != Engine::Snakemake)
            );
        }
        let docker = assignment(&[RuntimeKind::Docker]).await;
        assert_eq!(docker.len(), 4);
        assert!(
            docker
                .iter()
                .all(|(_, kind)| *kind == Some(RuntimeKind::Docker))
        );
    }
}
