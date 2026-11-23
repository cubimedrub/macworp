/// Target for tracing
///
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
pub enum TracingTarget {
    Loki,
    File,
    Terminal,
    All,
}
