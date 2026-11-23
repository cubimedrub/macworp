use std::io;

use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use crate::error::Error;

pub fn verbosity_from_count(count: u8) -> tracing::Level {
    match count {
        0 => tracing::Level::ERROR,
        1 => tracing::Level::WARN,
        2 => tracing::Level::INFO,
        3 => tracing::Level::DEBUG,
        _ => tracing::Level::TRACE,
    }
}

/// Initializes the tracing subscriber with the given verbosity level and optional additional directives.
///
/// # Arguments
/// * `verbosity_count` - The verbosity level as a count.
///     * 0 - Error
///     * 1 - Warn
///     * 2 - Info
///     * 3 - Debug
///     * 4 - Trace
///     * > 4 - Set to Trace and remove all filter directives for underlaying crates
/// * `target_terminal` - Whether to target the terminal for logging output
/// * `tracing_directives` - Additional tracing directives in the format `<crate>=<log level>`
pub fn init_tracing_subscription(
    verbosity_count: u8,
    target_terminal: bool,
    tracing_directives: &[&str],
) -> Result<(), Error> {
    let verbosity = verbosity_from_count(verbosity_count);
    let mut filter = EnvFilter::from_default_env().add_directive(verbosity.into());
    if verbosity_count <= 4 {
        for directive in tracing_directives {
            filter = filter.add_directive(
                directive
                    .parse()
                    .map_err(|_| Error::LoggingDirectiveParseing(directive.to_string()))?,
            );
        }
    }

    let mut terminal_layer = None;

    if target_terminal {
        terminal_layer = Some(tracing_subscriber::fmt::layer().with_writer(io::stderr));
    }

    tracing_subscriber::registry()
        .with(terminal_layer)
        .with(filter)
        .init();

    Ok(())
}
