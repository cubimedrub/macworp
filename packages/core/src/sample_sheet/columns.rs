use serde_json::Value;

use super::CellKind;

/// What a sample sheet column holds, which decides the input shown for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnKind {
    /// Free value of the given type
    Value(CellKind),
    /// One of a fixed list of values (`enum`)
    Choice(Vec<String>),
    /// Path of a file in the project (`format: file-path`)
    File,
    /// Path of a folder in the project (`format: directory-path`)
    Directory,
    /// Path of a file or folder in the project (`format: path`)
    Path,
}

/// One column of a sample sheet as described by its schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    /// Property name, the header in the file
    pub name: String,
    /// Listed in the schema's `required`
    pub required: bool,
    /// Kind of input
    pub kind: ColumnKind,
    /// Regular expression the value must match, as written in the schema
    pub pattern: Option<String>,
    /// Default value, as text
    pub default: Option<String>,
    /// Help text (`description`, else `errorMessage`)
    pub description: Option<String>,
}

impl Column {
    /// Type of the cells for JSON/YAML output.
    pub fn cell_kind(&self) -> CellKind {
        match self.kind {
            ColumnKind::Value(kind) => kind,
            _ => CellKind::Text,
        }
    }

    /// Derives the columns from a sample sheet schema (array of row objects, as in
    /// `assets/schema_input.json`). A schema that describes one row object directly is accepted
    /// too. Columns keep the order of the schema's properties.
    ///
    /// # Arguments
    /// * `schema` - the parsed schema document
    pub fn from_schema(schema: &Value) -> Vec<Column> {
        let row = schema.get("items").unwrap_or(schema);
        let required: Vec<&str> = row
            .get("required")
            .and_then(Value::as_array)
            .map(|names| names.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default();
        let Some(properties) = row.get("properties").and_then(Value::as_object) else {
            return Vec::new();
        };
        properties
            .iter()
            .map(|(name, property)| Column {
                name: name.clone(),
                required: required.contains(&name.as_str()),
                kind: kind_of(property),
                pattern: property
                    .get("pattern")
                    .and_then(Value::as_str)
                    .map(str::to_owned),
                default: property.get("default").and_then(scalar_text),
                description: property
                    .get("description")
                    .or_else(|| property.get("errorMessage"))
                    .and_then(Value::as_str)
                    .map(str::to_owned),
            })
            .collect()
    }
}

/// Text of a scalar JSON value.
fn scalar_text(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

/// Input kind of one property schema.
fn kind_of(property: &Value) -> ColumnKind {
    if let Some(values) = property.get("enum").and_then(Value::as_array) {
        return ColumnKind::Choice(values.iter().filter_map(scalar_text).collect());
    }
    match property.get("format").and_then(Value::as_str) {
        Some("file-path") => return ColumnKind::File,
        Some("directory-path") => return ColumnKind::Directory,
        Some("path") => return ColumnKind::Path,
        _ => {}
    }
    let type_name = match property.get("type") {
        Some(Value::String(name)) => Some(name.as_str()),
        Some(Value::Array(names)) => names
            .iter()
            .filter_map(Value::as_str)
            .find(|n| *n != "null"),
        _ => None,
    };
    ColumnKind::Value(match type_name {
        Some("integer") => CellKind::Integer,
        Some("number") => CellKind::Number,
        Some("boolean") => CellKind::Boolean,
        _ => CellKind::Text,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Columns of the nf-core/demo sheet schema: order, required, kinds, pattern.
    #[test]
    fn nf_core_demo_columns() {
        let schema: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/nf_core_demo_schema_input.json"
        ))
        .unwrap();
        let columns = Column::from_schema(&schema);
        let names: Vec<&str> = columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["sample", "fastq_1", "fastq_2"]);
        assert!(columns[0].required && columns[1].required && !columns[2].required);
        assert_eq!(columns[0].kind, ColumnKind::Value(CellKind::Text));
        assert_eq!(columns[1].kind, ColumnKind::File);
        assert_eq!(columns[0].pattern.as_deref(), Some("^\\S+$"));
        assert!(
            columns[1]
                .description
                .as_deref()
                .unwrap()
                .contains("reads 1")
        );
    }

    /// Enums, numbers, nullable types, defaults and a bare row schema.
    #[test]
    fn other_kinds() {
        let schema: Value = serde_json::json!({
            "properties": {
                "strandedness": {"enum": ["auto", "forward"], "default": "auto"},
                "depth": {"type": ["null", "integer"]},
                "ratio": {"type": "number"},
                "keep": {"type": "boolean", "default": true},
                "folder": {"type": "string", "format": "directory-path"}
            }
        });
        let columns = Column::from_schema(&schema);
        assert_eq!(
            columns[0].kind,
            ColumnKind::Choice(vec!["auto".into(), "forward".into()])
        );
        assert_eq!(columns[0].default.as_deref(), Some("auto"));
        assert_eq!(columns[1].kind, ColumnKind::Value(CellKind::Integer));
        assert_eq!(columns[2].cell_kind(), CellKind::Number);
        assert_eq!(columns[3].default.as_deref(), Some("true"));
        assert_eq!(columns[4].kind, ColumnKind::Directory);
        assert!(Column::from_schema(&Value::Null).is_empty());
    }
}
