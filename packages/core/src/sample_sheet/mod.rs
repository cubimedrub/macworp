//! Sample sheets: the table model, reading and writing of the file formats nf-core style
//! pipelines accept (CSV, TSV, JSON, YAML), column derivation from a schema,
//! editing and turning the files of a folder into rows. Pure data handling, so it compiles for the browser as well as natively.

mod columns;
mod delimited;
mod editing;
mod sample_files;
mod sheet;
mod validation;

pub use columns::{Column, ColumnKind};
pub use editing::ListingChecker;
pub use sample_files::SampleFiles;
pub use sheet::{CellKind, Error, Format, Sheet};
