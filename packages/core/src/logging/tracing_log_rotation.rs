/// Log rotation values for CLI
///
#[derive(clap::ValueEnum, Clone, Debug)]
enum TracingLogRotation {
    Minutely,
    Hourly,
    Daily,
    Never,
}

impl From<TracingLogRotation> for Rotation {
    fn from(rotation: TracingLogRotation) -> Self {
        match rotation {
            TracingLogRotation::Minutely => Rotation::MINUTELY,
            TracingLogRotation::Hourly => Rotation::HOURLY,
            TracingLogRotation::Daily => Rotation::DAILY,
            TracingLogRotation::Never => Rotation::NEVER,
        }
    }
}
