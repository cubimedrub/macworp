use std::path::Path;

use serde_json::{Map, Number, Value};

use super::delimited::{parse_records, write_records};

/// Errors while reading or writing a sample sheet.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The text is not valid in the chosen format.
    #[error("Invalid {format} sample sheet: {message}")]
    Invalid {
        /// Name of the format
        format: &'static str,
        /// What is wrong
        message: String,
    },
    /// The file extension is not a known sample sheet format.
    #[error("Unknown sample sheet format: {0}")]
    UnknownFormat(String),
}

/// File formats a sample sheet can be stored in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Comma separated values
    Csv,
    /// Tab separated values
    Tsv,
    /// JSON array of row objects
    Json,
    /// YAML sequence of row mappings
    Yaml,
}

impl Format {
    /// Name used in messages.
    pub fn name(self) -> &'static str {
        match self {
            Format::Csv => "CSV",
            Format::Tsv => "TSV",
            Format::Json => "JSON",
            Format::Yaml => "YAML",
        }
    }

    /// Format for a file name, by extension (case-insensitive).
    ///
    /// # Arguments
    /// * `path` - file name or path
    pub fn from_path(path: &Path) -> Result<Format, Error> {
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        match extension.as_str() {
            "csv" => Ok(Format::Csv),
            "tsv" | "txt" => Ok(Format::Tsv),
            "json" => Ok(Format::Json),
            "yaml" | "yml" => Ok(Format::Yaml),
            _ => Err(Error::UnknownFormat(extension)),
        }
    }
}

/// How a cell is typed in the structured formats (JSON, YAML). Delimited formats only know text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CellKind {
    /// Text, written as a string
    #[default]
    Text,
    /// Whole number
    Integer,
    /// Any number
    Number,
    /// `true` / `false`
    Boolean,
}

/// A sample sheet: named columns and rows of text cells. An empty cell means "not set".
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Sheet {
    /// Column names in order
    pub columns: Vec<String>,
    /// Rows; every row has exactly one cell per column
    pub rows: Vec<Vec<String>>,
}

impl Sheet {
    /// Parses a sheet. Ragged delimited rows are padded or rejected (too many cells).
    ///
    /// # Arguments
    /// * `format` - format of `text`
    /// * `text` - file content
    pub fn parse(format: Format, text: &str) -> Result<Sheet, Error> {
        let invalid = |message: String| Error::Invalid {
            format: format.name(),
            message,
        };
        match format {
            Format::Csv | Format::Tsv => {
                let delimiter = if format == Format::Csv { ',' } else { '\t' };
                let mut records = parse_records(text, delimiter).map_err(invalid)?.into_iter();
                let columns = records.next().unwrap_or_default();
                let mut rows = Vec::new();
                for (index, mut record) in records.enumerate() {
                    if record.len() > columns.len() {
                        return Err(invalid(format!(
                            "row {} has {} cells but there are {} columns",
                            index + 1,
                            record.len(),
                            columns.len()
                        )));
                    }
                    record.resize(columns.len(), String::new());
                    rows.push(record);
                }
                Ok(Sheet { columns, rows })
            }
            Format::Json => {
                let value: Value =
                    serde_json::from_str(text).map_err(|error| invalid(error.to_string()))?;
                Sheet::from_value(&value).map_err(invalid)
            }
            Format::Yaml => {
                let value: Value =
                    serde_norway::from_str(text).map_err(|error| invalid(error.to_string()))?;
                Sheet::from_value(&value).map_err(invalid)
            }
        }
    }

    /// Writes the sheet.
    ///
    /// # Arguments
    /// * `format` - target format
    /// * `kinds` - cell types per column for JSON/YAML (missing entries mean text); ignored
    ///   for delimited formats
    pub fn serialize(&self, format: Format, kinds: &[CellKind]) -> String {
        match format {
            Format::Csv | Format::Tsv => {
                let delimiter = if format == Format::Csv { ',' } else { '\t' };
                let mut records = vec![self.columns.clone()];
                records.extend(self.rows.iter().cloned());
                write_records(&records, delimiter)
            }
            Format::Json => {
                serde_json::to_string_pretty(&self.to_value(kinds)).unwrap_or_default() + "\n"
            }
            Format::Yaml => serde_norway::to_string(&self.to_value(kinds)).unwrap_or_default(),
        }
    }

    /// The sheet as an array of row objects, ready for schema validation. Empty cells are left
    /// out; cells that do not read as their column's type stay strings so validation reports them.
    ///
    /// # Arguments
    /// * `kinds` - cell types per column (missing entries mean text)
    pub fn to_value(&self, kinds: &[CellKind]) -> Value {
        Value::Array(
            self.rows
                .iter()
                .map(|row| {
                    let mut object = Map::new();
                    for (index, (column, cell)) in self.columns.iter().zip(row).enumerate() {
                        if !cell.is_empty() {
                            let kind = kinds.get(index).copied().unwrap_or_default();
                            object.insert(column.clone(), typed(cell, kind));
                        }
                    }
                    Value::Object(object)
                })
                .collect(),
        )
    }

    /// Builds a sheet from an array of row objects. Columns are the keys in order of first
    /// appearance; scalars become text, nested values are rejected.
    ///
    /// # Arguments
    /// * `value` - the parsed JSON/YAML document
    fn from_value(value: &Value) -> Result<Sheet, String> {
        let Value::Array(items) = value else {
            return Err("expected a list of rows".to_string());
        };
        let mut columns: Vec<String> = Vec::new();
        for item in items {
            let Value::Object(object) = item else {
                return Err("every row must be an object".to_string());
            };
            for key in object.keys() {
                if !columns.contains(key) {
                    columns.push(key.clone());
                }
            }
        }
        let mut rows = Vec::new();
        for item in items {
            let Value::Object(object) = item else {
                unreachable!("checked above")
            };
            let mut row = Vec::with_capacity(columns.len());
            for column in &columns {
                row.push(match object.get(column) {
                    None | Some(Value::Null) => String::new(),
                    Some(Value::String(text)) => text.clone(),
                    Some(Value::Bool(flag)) => flag.to_string(),
                    Some(Value::Number(number)) => number.to_string(),
                    Some(_) => return Err(format!("column {column} holds a nested value")),
                });
            }
            rows.push(row);
        }
        Ok(Sheet { columns, rows })
    }
}

/// Converts a cell to its JSON type, falling back to a string when it does not parse.
fn typed(cell: &str, kind: CellKind) -> Value {
    let trimmed = cell.trim();
    match kind {
        CellKind::Text => Value::String(cell.to_string()),
        CellKind::Integer => trimmed
            .parse::<i64>()
            .map(Value::from)
            .unwrap_or_else(|_| Value::String(cell.to_string())),
        CellKind::Number => trimmed
            .parse::<f64>()
            .ok()
            .and_then(Number::from_f64)
            .map(Value::Number)
            .unwrap_or_else(|| Value::String(cell.to_string())),
        CellKind::Boolean => match trimmed {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => Value::String(cell.to_string()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet() -> Sheet {
        Sheet {
            columns: vec![
                "sample".into(),
                "fastq_1".into(),
                "count".into(),
                "ok".into(),
            ],
            rows: vec![
                vec!["A".into(), "a_R1.fq.gz".into(), "3".into(), "true".into()],
                vec!["B".into(), "".into(), "".into(), "".into()],
            ],
        }
    }

    const KINDS: [CellKind; 4] = [
        CellKind::Text,
        CellKind::Text,
        CellKind::Integer,
        CellKind::Boolean,
    ];

    /// Every format reads back what it wrote.
    #[test]
    fn round_trips() {
        for format in [Format::Csv, Format::Tsv, Format::Json, Format::Yaml] {
            let text = sheet().serialize(format, &KINDS);
            let mut parsed = Sheet::parse(format, &text).unwrap();
            if matches!(format, Format::Json | Format::Yaml) {
                // empty cells are omitted, so the last row only keeps the sample column;
                // columns are still found through the first row
                assert_eq!(parsed.columns, sheet().columns);
            }
            parsed.columns = sheet().columns;
            assert_eq!(parsed, sheet(), "{format:?}");
        }
    }

    /// Typed output for structured formats, strings when the cell does not parse.
    #[test]
    fn typed_values() {
        let value = sheet().to_value(&KINDS);
        assert_eq!(value[0]["count"], Value::from(3));
        assert_eq!(value[0]["ok"], Value::Bool(true));
        assert!(value[1].get("count").is_none());
        let mut bad = sheet();
        bad.rows[0][2] = "three".into();
        assert_eq!(
            bad.to_value(&KINDS)[0]["count"],
            Value::String("three".into())
        );
    }

    /// Short delimited rows are padded, long ones rejected, empty text is an empty sheet.
    #[test]
    fn ragged_rows() {
        let parsed = Sheet::parse(Format::Csv, "a,b\n1\n").unwrap();
        assert_eq!(parsed.rows, vec![vec!["1".to_string(), String::new()]]);
        assert!(Sheet::parse(Format::Csv, "a\n1,2\n").is_err());
        assert_eq!(Sheet::parse(Format::Csv, "").unwrap(), Sheet::default());
    }

    /// Structured input must be a list of flat objects.
    #[test]
    fn rejects_bad_structures() {
        assert!(Sheet::parse(Format::Json, "{}").is_err());
        assert!(Sheet::parse(Format::Json, "[1]").is_err());
        assert!(Sheet::parse(Format::Json, r#"[{"a":{"b":1}}]"#).is_err());
        assert!(Sheet::parse(Format::Yaml, "- a: 1\n  b: [1]\n").is_err());
    }

    /// Format by extension.
    #[test]
    fn format_from_path() {
        assert_eq!(
            Format::from_path(Path::new("x/S.CSV")).unwrap(),
            Format::Csv
        );
        assert_eq!(Format::from_path(Path::new("s.yml")).unwrap(), Format::Yaml);
        assert!(Format::from_path(Path::new("s.xlsx")).is_err());
    }
}
