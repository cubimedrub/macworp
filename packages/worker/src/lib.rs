/// Worker configuration
pub mod configuration;
/// Detection and access to the container runtime (podman/docker) the engines run in
pub mod container_runtime;
/// Workflow executor, for building and running the workflow using an external tool
pub mod executors;
/// Proxy for catching workflow engine logs and sending them to the backend for further processing
pub mod log_proxy;
