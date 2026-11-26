/// Trimming of the Apptainer image cache Snakemake fills
pub mod apptainer_cache_store;
/// Worker configuration
pub mod configuration;
/// Detection and access to the container runtime (podman/docker) the engines run in
/// Keeps child processes from opening console windows on Windows
pub mod console_window;
pub mod container_runtime;
/// Workflow executor, for building and running the workflow using an external tool
pub mod executors;
/// Job source of remote workers: claims and reports over HTTP
pub mod http_job_source;
/// Builds the container images a workflow brings along before its engine starts
pub mod image_builder;
/// Usage tracking and garbage collection of the container images a worker accumulates
pub mod image_store;
/// Where a worker gets jobs from and reports to
pub mod job_source;
/// HTTP receiver for the monitoring events of the engines (weblog / Snakemake panoptes logger)
pub mod log_proxy;
/// Turns engine monitoring events (Nextflow weblog, Snakemake panoptes logger plugin) into progress numbers
pub mod progress;
/// SELinux labels of the directories MAcWorP owns
pub mod selinux;
/// Keeps the computer awake while runs execute on mains power
pub mod sleep_prevention;
/// Claims jobs and runs them with the executor of their engine
pub mod worker;
