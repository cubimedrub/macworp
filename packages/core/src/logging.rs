use std::io;

use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

pub fn verbosity_from_count(count: u8) -> tracing::Level {
    match count {
        0 => tracing::Level::ERROR,
        1 => tracing::Level::WARN,
        2 => tracing::Level::INFO,
        3 => tracing::Level::DEBUG,
        _ => tracing::Level::TRACE,
    }
}

pub fn init_tracing_subscription(verbosity_count: u8, target_terminal: bool) {
    let verbosity = verbosity_from_count(verbosity_count);
    let filter = EnvFilter::from_default_env().add_directive(verbosity.into());

    let mut terminal_layer = None;

    if target_terminal {
        terminal_layer = Some(tracing_subscriber::fmt::layer().with_writer(io::stderr));
    }

    tracing_subscriber::registry()
        .with(terminal_layer)
        .with(filter)
        .init();
}
