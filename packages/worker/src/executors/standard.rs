use std::sync::Arc;

use super::{
    Executor,
    container::ContainerExecutor,
    nextflow::{self, NextflowExecutor},
    snakemake::{self, SnakemakeExecutor},
};
use crate::container_runtime::ContainerRuntime;

/// The executors of all engines `runtime` can run, wired to start sibling containers through
/// the runtime socket when it has one (engines that cannot run on the runtime are left out).
pub async fn standard_executors(runtime: &Arc<dyn ContainerRuntime>) -> Vec<Arc<dyn Executor>> {
    let binary = runtime.binary().to_path_buf();
    let engine_access = match runtime.info().await {
        Ok(info) => info.engine_access_args().unwrap_or_default(),
        Err(e) => {
            tracing::warn!("Unable to query container runtime: {e}");
            Vec::new()
        }
    };
    if engine_access.is_empty() {
        tracing::warn!("No runtime socket found, engines cannot start sibling containers");
    }
    let executors: Vec<Arc<dyn Executor>> = vec![
        Arc::new(
            NextflowExecutor::new(&binary, nextflow::DEFAULT_IMAGE)
                .with_engine_access(engine_access.clone()),
        ),
        Arc::new(
            SnakemakeExecutor::new(&binary, snakemake::DEFAULT_IMAGE)
                .with_engine_access(engine_access),
        ),
        Arc::new(ContainerExecutor::new(&binary)),
    ];
    executors
        .into_iter()
        .filter(|executor| runtime.supports_engine(&executor.engine()))
        .collect()
}
