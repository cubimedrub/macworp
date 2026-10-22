/// Worker configuration
pub mod configuration;
/// Detection and access to the container runtime (podman/docker) the engines run in
pub mod container_runtime;
/// Workflow executor, for building and running the workflow using an external tool
pub mod executors;
/// Where a worker gets jobs from and reports to
pub mod job_source;
/// Proxy for catching workflow engine logs and sending them to the backend for further processing
pub mod log_proxy;
/// Claims jobs and runs them with the executor of their engine
pub mod worker;
