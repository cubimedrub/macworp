use clap::Args;

use crate::container_runtime::RuntimePreference;

/// Worker configuration, usable as clap arguments (worker binary) or built directly (embedded worker)
#[derive(Debug, Clone, Default, Args)]
pub struct WorkerConfiguration {
    /// Container runtime to use for running workflow engines
    #[arg(
        long,
        env = "MACWORP_CONTAINER_RUNTIME",
        value_enum,
        default_value_t = RuntimePreference::Auto
    )]
    pub container_runtime: RuntimePreference,
}
