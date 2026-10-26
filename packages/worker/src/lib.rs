/// Worker configuration
pub mod configuration;
/// Detection and access to the container runtime (podman/docker) the engines run in
pub mod container_runtime;
/// Workflow executor, for building and running the workflow using an external tool
pub mod executors;
/// Where a worker gets jobs from and reports to
pub mod job_source;
/// HTTP receiver for the monitoring events of the engines (weblog / wms-monitor)
pub mod log_proxy;
/// Turns engine monitoring events (Nextflow weblog, Snakemake wms-monitor) into progress numbers
pub mod progress;
/// Claims jobs and runs them with the executor of their engine
pub mod worker;
