/// Executor for `Container` workflows: runs the workflow's image directly
pub mod container;
/// Builds and runs the `<runtime> run ...` command shared by all executors
mod containerized_execution;
/// The `Executor` trait implemented per workflow engine, and its error
mod executor;
/// Types describing a claimed run and how it ended
mod job;
/// Executor for Nextflow, runs the engine inside an engine image
pub mod nextflow;
/// Starts a process, forwards its output and stops it on cancel
mod process;
/// Executor for Snakemake, runs the engine inside an engine image
pub mod snakemake;
/// Executors of all engines a container runtime supports
mod standard;

pub use containerized_execution::{containerized_spec, run_spec};
pub use executor::{Error as ExecutorError, Executor};
pub use job::{Job, LogLine, Outcome};
pub use process::{LaunchSpec, run_process};
pub use standard::standard_executors;
