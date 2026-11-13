//! Sample sheets: the table model, reading and writing of the file formats nf-core style
//! pipelines accept (CSV, TSV, JSON, YAML), column derivation from a schema and pairing of
//! read files. Pure data handling, so it compiles for the browser as well as natively.

mod columns;
mod delimited;
mod pairing;
mod sheet;
mod validation;

pub use columns::{Column, ColumnKind};
pub use pairing::ReadPair;
pub use sheet::{CellKind, Error, Format, Sheet};
