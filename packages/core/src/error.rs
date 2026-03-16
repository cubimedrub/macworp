use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error(
        "Unable to parse logging directive `{0}`. Is everything in format `<crate>=<loglevel>?."
    )]
    LoggingDirectiveParseing(String),
}
