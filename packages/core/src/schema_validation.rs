//! JSON Schema validation shared by the UI (WASM, live feedback) and the worker (native, check
//! before a run). On top of plain JSON Schema it understands the nf-schema extensions that
//! sample sheet schemas (`assets/schema_input.json`) use: the path formats, `exists`, `unique`
//! and `errorMessage`. Nothing here touches the filesystem: whether a path exists is answered by
//! an injected [`PathChecker`], so the browser can use the project file listing and the worker
//! the real directory.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use jsonschema::{Draft, Validator, error::ValidationErrorKind};
use serde_json::{Map, Value};

use crate::paths::is_confined;

/// Format names that denote a path inside the project (or the reference data). They are checked
/// lexically with [`is_confined`].
const PATH_FORMATS: [&str; 4] = ["file-path", "directory-path", "path", "file-path-pattern"];

/// Errors raised while preparing a schema.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The schema itself is not a valid JSON Schema of the chosen dialect.
    #[error("Invalid schema: {0}")]
    InvalidSchema(String),
    /// The schema declares a `$schema` that is not a known JSON Schema draft (or not a string).
    /// The validation crate would silently treat it as the latest draft, which could report
    /// invalid data as valid, so this is an error instead.
    #[error("Unsupported JSON Schema dialect: {0}")]
    UnsupportedDialect(String),
}

/// What a path value is expected to point to when `exists` is requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathKind {
    /// Regular file (`format: file-path`).
    File,
    /// Directory (`format: directory-path`).
    Directory,
    /// File or directory (`format: path`).
    Any,
}

/// Answers whether a project path exists. Implemented by whoever knows the data: the web UI
/// (project file listing) or the worker (real filesystem).
pub trait PathChecker {
    /// Whether `path` exists.
    ///
    /// # Arguments
    /// * `path` - lexically sanitised path relative to the project (reference data keeps its
    ///   virtual prefix, see `paths::REFERENCE_PREFIX`)
    /// * `kind` - what the path has to be
    fn exists(&self, path: &str, kind: PathKind) -> bool;
}

/// A [`PathChecker`] for contexts that cannot look at files: every path exists.
#[derive(Debug, Clone, Copy, Default)]
pub struct AssumeExisting;

impl PathChecker for AssumeExisting {
    fn exists(&self, _path: &str, _kind: PathKind) -> bool {
        true
    }
}

/// One validation problem, located so an editor can mark the cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    /// JSON pointer to the offending value (empty for the document itself).
    pub instance_path: String,
    /// Zero-based row, when the document is an array (a sample sheet).
    pub row: Option<usize>,
    /// Column (property name) of the offending value, when known.
    pub column: Option<String>,
    /// Schema keyword that failed (`required`, `pattern`, `unique`, `exists`, ...).
    pub keyword: String,
    /// Human readable message; the `errorMessage` of the property when the schema has one.
    pub message: String,
}

/// A compiled schema plus the nf-schema extensions that plain JSON Schema does not know.
pub struct SchemaValidator {
    /// The schema as given, kept to read `errorMessage`, `exists` and `unique`.
    schema: Value,
    /// Compiled validator of the underlying crate.
    validator: Validator,
}

impl SchemaValidator {
    /// Compiles `schema` in the draft it declares in `$schema` (draft 4, 6, 7, 2019-09 or
    /// 2020-12, as far as the validation crate supports them). A schema without `$schema` is
    /// treated as the latest draft, 2020-12.
    ///
    /// # Arguments
    /// * `schema` - the JSON Schema (for sample sheets: the `schema_input.json` content)
    ///
    /// # Errors
    /// [`Error::UnsupportedDialect`] when `$schema` is not a string or names an unknown draft,
    /// [`Error::InvalidSchema`] when the schema does not compile.
    pub fn new(schema: &Value) -> Result<Self, Error> {
        if let Some(declared) = schema.get("$schema") {
            let known = declared
                .as_str()
                .is_some_and(|uri| Draft::from_schema_uri(uri) != Draft::Unknown);
            if !known {
                return Err(Error::UnsupportedDialect(declared.to_string()));
            }
        }
        let mut options = jsonschema::options().should_validate_formats(true);
        for format in PATH_FORMATS {
            options = options.with_format(format, |value: &str| is_confined(Path::new(value)));
        }
        let validator = options
            .build(schema)
            .map_err(|error| Error::InvalidSchema(error.to_string()))?;
        Ok(Self {
            schema: schema.clone(),
            validator,
        })
    }

    /// Validates `instance` and returns every problem found (empty = valid).
    ///
    /// # Arguments
    /// * `instance` - the document, for a sample sheet an array of row objects
    /// * `paths` - decides whether paths marked `exists` are present
    pub fn validate(&self, instance: &Value, paths: &dyn PathChecker) -> Vec<ValidationIssue> {
        let mut issues: Vec<ValidationIssue> = self
            .validator
            .iter_errors(instance)
            .map(|error| {
                let property = match error.kind() {
                    ValidationErrorKind::Required { property } => {
                        property.as_str().map(str::to_owned)
                    }
                    _ => None,
                };
                let instance_path = error.instance_path().to_string();
                let mut issue = self.issue(
                    instance,
                    instance_path,
                    error.kind().keyword().to_owned(),
                    error.to_string(),
                );
                if let Some(property) = property {
                    // A missing property is reported on the row; point at its column instead.
                    issue.column = Some(property);
                }
                if let Some(message) = self.error_message(instance, &issue) {
                    issue.message = message;
                }
                issue
            })
            .collect();
        if let (Some(rows), Some(properties)) = (instance.as_array(), self.row_properties()) {
            self.check_exists(rows, properties, paths, &mut issues);
            self.check_unique(rows, properties, &mut issues);
        }
        issues
    }

    /// Builds an issue, deriving row and column from the JSON pointer.
    ///
    /// # Arguments
    /// * `instance` - the validated document (tells whether the first segment is a row)
    /// * `instance_path` - JSON pointer of the value
    /// * `keyword` - failed schema keyword
    /// * `message` - message to show
    fn issue(
        &self,
        instance: &Value,
        instance_path: String,
        keyword: String,
        message: String,
    ) -> ValidationIssue {
        let mut segments = instance_path.split('/').skip(1);
        let (row, column) = if instance.is_array() {
            (
                segments.next().and_then(|s| s.parse().ok()),
                segments.next().map(str::to_owned),
            )
        } else {
            (None, segments.next().map(str::to_owned))
        };
        ValidationIssue {
            instance_path,
            row,
            column,
            keyword,
            message,
        }
    }

    /// The `properties` of the row schema (`items.properties`), if the schema describes an array
    /// of objects.
    fn row_properties(&self) -> Option<&Map<String, Value>> {
        self.schema.get("items")?.get("properties")?.as_object()
    }

    /// The `errorMessage` declared for the column of `issue`, if any.
    ///
    /// # Arguments
    /// * `instance` - the validated document; only array documents carry per-column messages
    /// * `issue` - the problem to find a message for
    fn error_message(&self, instance: &Value, issue: &ValidationIssue) -> Option<String> {
        if !instance.is_array() {
            return None;
        }
        self.row_properties()?
            .get(issue.column.as_deref()?)?
            .get("errorMessage")?
            .as_str()
            .map(str::to_owned)
    }

    /// Reports string values of `exists: true` path columns that the checker does not know.
    /// Values that fail the lexical path check are skipped (the format error already covers
    /// them), as are `file-path-pattern` columns (globs have no single path to look up).
    ///
    /// # Arguments
    /// * `rows` - the sample sheet rows
    /// * `properties` - column schemas
    /// * `paths` - existence oracle
    /// * `issues` - receives the problems
    fn check_exists(
        &self,
        rows: &[Value],
        properties: &Map<String, Value>,
        paths: &dyn PathChecker,
        issues: &mut Vec<ValidationIssue>,
    ) {
        for (column, definition) in properties {
            if definition.get("exists") != Some(&Value::Bool(true)) {
                continue;
            }
            let kind = match definition.get("format").and_then(Value::as_str) {
                Some("file-path") => PathKind::File,
                Some("directory-path") => PathKind::Directory,
                Some("path") => PathKind::Any,
                _ => continue,
            };
            for (row, value) in rows.iter().enumerate() {
                let Some(text) = value.get(column).and_then(Value::as_str) else {
                    continue;
                };
                if text.is_empty() || !is_confined(Path::new(text)) {
                    continue;
                }
                let sanitized = crate::paths::sanitize_relative(Path::new(text));
                if !paths.exists(&sanitized.to_string_lossy(), kind) {
                    let message = definition
                        .get("errorMessage")
                        .and_then(Value::as_str)
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("'{text}' does not exist"));
                    issues.push(ValidationIssue {
                        instance_path: format!("/{row}/{column}"),
                        row: Some(row),
                        column: Some(column.clone()),
                        keyword: "exists".to_owned(),
                        message,
                    });
                }
            }
        }
    }

    /// Reports duplicates in `unique` columns. `unique: true` makes the column unique on its own,
    /// `unique: ["other"]` makes the combination of the column and the listed columns unique.
    /// Rows without a value in the column are ignored. The second and later occurrence is
    /// reported.
    ///
    /// # Arguments
    /// * `rows` - the sample sheet rows
    /// * `properties` - column schemas
    /// * `issues` - receives the problems
    fn check_unique(
        &self,
        rows: &[Value],
        properties: &Map<String, Value>,
        issues: &mut Vec<ValidationIssue>,
    ) {
        for (column, definition) in properties {
            let mut columns = vec![column.clone()];
            match definition.get("unique") {
                Some(Value::Bool(true)) => {}
                Some(Value::Array(others)) => {
                    columns.extend(others.iter().filter_map(Value::as_str).map(str::to_owned));
                }
                _ => continue,
            }
            let mut seen: HashSet<Vec<String>> = HashSet::new();
            let mut first_row: HashMap<Vec<String>, usize> = HashMap::new();
            for (row, value) in rows.iter().enumerate() {
                if value.get(column).is_none() {
                    continue;
                }
                let key: Vec<String> = columns
                    .iter()
                    .map(|name| value.get(name).map(Value::to_string).unwrap_or_default())
                    .collect();
                if seen.insert(key.clone()) {
                    first_row.insert(key, row);
                    continue;
                }
                let message = definition
                    .get("errorMessage")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .unwrap_or_else(|| {
                        format!(
                            "'{column}' must be unique, row {} repeats row {}",
                            row + 1,
                            first_row[&key] + 1
                        )
                    });
                issues.push(ValidationIssue {
                    instance_path: format!("/{row}/{column}"),
                    row: Some(row),
                    column: Some(column.clone()),
                    keyword: "unique".to_owned(),
                    message,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The `schema_input.json` of nf-core/demo, the reference sample sheet schema.
    const DEMO_SCHEMA: &str = include_str!("../tests/fixtures/nf_core_demo_schema_input.json");

    /// Path checker backed by a fixed set of existing paths.
    struct Known(HashSet<&'static str>);

    impl PathChecker for Known {
        fn exists(&self, path: &str, _kind: PathKind) -> bool {
            self.0.contains(path)
        }
    }

    /// Validator for the nf-core/demo sample sheet schema.
    fn demo() -> SchemaValidator {
        let schema: Value = serde_json::from_str(DEMO_SCHEMA).unwrap();
        SchemaValidator::new(&schema).unwrap()
    }

    /// Checker that knows the two files used by the valid demo row.
    fn demo_files() -> Known {
        Known(HashSet::from(["data/a_1.fastq.gz", "data/a_2.fastq.gz"]))
    }

    /// A correct row passes.
    #[test]
    fn valid_sheet_has_no_issues() {
        let sheet = json!([{"sample": "a", "fastq_1": "data/a_1.fastq.gz", "fastq_2": "data/a_2.fastq.gz"}]);
        assert!(demo().validate(&sheet, &demo_files()).is_empty());
    }

    /// A missing required column is reported on its column with the schema's own message.
    #[test]
    fn missing_required_column_uses_error_message() {
        let sheet = json!([{"sample": "a"}]);
        let issues = demo().validate(&sheet, &demo_files());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].row, Some(0));
        assert_eq!(issues[0].column.as_deref(), Some("fastq_1"));
        assert_eq!(issues[0].keyword, "required");
        assert!(issues[0].message.starts_with("FastQ file for reads 1"));
    }

    /// A pattern violation carries the `errorMessage` of its column.
    #[test]
    fn pattern_violation_uses_error_message() {
        let sheet = json!([{"sample": "has space", "fastq_1": "data/a_1.fastq.gz"}]);
        let issues = demo().validate(&sheet, &demo_files());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].column.as_deref(), Some("sample"));
        assert_eq!(
            issues[0].message,
            "Sample name must be provided and cannot contain spaces"
        );
    }

    /// `exists: true` asks the injected checker; unknown files are reported, once per cell.
    #[test]
    fn missing_file_is_reported() {
        let sheet = json!([{"sample": "a", "fastq_1": "data/missing.fastq.gz"}]);
        let issues = demo().validate(&sheet, &demo_files());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].keyword, "exists");
        assert_eq!(issues[0].instance_path, "/0/fastq_1");
    }

    /// With the permissive checker only schema errors remain.
    #[test]
    fn assume_existing_skips_the_file_lookup() {
        let sheet = json!([{"sample": "a", "fastq_1": "data/missing.fastq.gz"}]);
        assert!(demo().validate(&sheet, &AssumeExisting).is_empty());
    }

    /// Paths that leave the project fail the path format, and are not looked up.
    #[test]
    fn escaping_path_is_rejected_by_the_format() {
        let sheet = json!([{"sample": "a", "fastq_1": "../secret.fastq.gz"}]);
        let issues = demo().validate(&sheet, &demo_files());
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].keyword, "format");
    }

    /// `unique: true` reports the repeated row, not the first one.
    #[test]
    fn unique_column_reports_the_repeat() {
        let schema = json!({
            "type": "array",
            "items": {"type": "object", "properties": {"sample": {"type": "string", "unique": true}}}
        });
        let validator = SchemaValidator::new(&schema).unwrap();
        let issues = validator.validate(
            &json!([{"sample": "a"}, {"sample": "b"}, {"sample": "a"}]),
            &AssumeExisting,
        );
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].row, Some(2));
        assert_eq!(issues[0].keyword, "unique");
    }

    /// `unique: [other]` makes the combination unique, so the same sample with another lane is fine.
    #[test]
    fn unique_with_other_columns_checks_the_combination() {
        let schema = json!({
            "type": "array",
            "items": {"type": "object", "properties": {
                "sample": {"type": "string", "unique": ["lane"]},
                "lane": {"type": "string"}
            }}
        });
        let validator = SchemaValidator::new(&schema).unwrap();
        let sheet = json!([
            {"sample": "a", "lane": "1"},
            {"sample": "a", "lane": "2"},
            {"sample": "a", "lane": "1"}
        ]);
        let issues = validator.validate(&sheet, &AssumeExisting);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].row, Some(2));
    }

    /// `dependentRequired` (2020-12) is plain JSON Schema and works through the crate.
    #[test]
    fn dependent_required_is_enforced() {
        let schema = json!({
            "type": "array",
            "items": {"type": "object",
                "properties": {"fastq_2": {"type": "string"}, "fastq_1": {"type": "string"}},
                "dependentRequired": {"fastq_2": ["fastq_1"]}}
        });
        let validator = SchemaValidator::new(&schema).unwrap();
        let issues = validator.validate(&json!([{"fastq_2": "x"}]), &AssumeExisting);
        assert_eq!(issues.len(), 1);
        // The crate reports a failed dependency as a missing required property.
        assert_eq!(issues[0].keyword, "required");
        assert_eq!(issues[0].column.as_deref(), Some("fastq_1"));
    }

    /// Every draft the validation crate knows is recognised from `$schema` (with or without
    /// `https` and the trailing `#`).
    #[test]
    fn all_known_drafts_compile() {
        for uri in [
            "http://json-schema.org/draft-04/schema#",
            "http://json-schema.org/draft-06/schema#",
            "http://json-schema.org/draft-07/schema#",
            "https://json-schema.org/draft-07/schema",
            "https://json-schema.org/draft/2019-09/schema",
            "https://json-schema.org/draft/2020-12/schema",
        ] {
            assert!(
                SchemaValidator::new(&json!({ "$schema": uri })).is_ok(),
                "{uri}"
            );
        }
    }

    /// Unknown drafts and malformed `$schema` values are rejected instead of being validated as
    /// the latest draft.
    #[test]
    fn unknown_dialects_are_rejected() {
        for declared in [
            json!("https://example.org/own-meta-schema"),
            json!("http://json-schema.org/draft-03/schema#"),
            json!(7),
        ] {
            let result = SchemaValidator::new(&json!({ "$schema": declared }));
            assert!(matches!(result, Err(Error::UnsupportedDialect(_))));
        }
    }

    /// A draft 4 schema (boolean `exclusiveMinimum`) compiles and validates, as used by
    /// published Snakemake workflows.
    #[test]
    fn draft_four_schema_validates() {
        let schema = json!({
            "$schema": "http://json-schema.org/draft-04/schema#",
            "type": "object",
            "properties": {"threads": {"type": "integer", "minimum": 0, "exclusiveMinimum": true}},
            "required": ["threads"]
        });
        let validator = SchemaValidator::new(&schema).unwrap();
        assert!(
            validator
                .validate(&json!({"threads": 4}), &AssumeExisting)
                .is_empty()
        );
        assert_eq!(
            validator
                .validate(&json!({"threads": 0}), &AssumeExisting)
                .len(),
            1
        );
    }

    /// The declared draft decides which keywords apply: `dependentRequired` exists only from
    /// 2019-09 on, so a draft 7 schema must not enforce it. Without `$schema` the latest draft
    /// applies.
    #[test]
    fn declared_draft_decides_which_keywords_apply() {
        let body = json!({
            "type": "object",
            "properties": {"a": {"type": "string"}, "b": {"type": "string"}},
            "dependentRequired": {"a": ["b"]}
        });
        let sheet = json!({"a": "x"});
        let issues = |declared: Option<&str>| {
            let mut schema = body.clone();
            if let Some(uri) = declared {
                schema["$schema"] = json!(uri);
            }
            SchemaValidator::new(&schema)
                .unwrap()
                .validate(&sheet, &AssumeExisting)
                .len()
        };
        assert_eq!(issues(Some("http://json-schema.org/draft-07/schema#")), 0);
        assert_eq!(
            issues(Some("https://json-schema.org/draft/2020-12/schema")),
            1
        );
        assert_eq!(issues(None), 1);
    }

    /// Draft 7 schemas compile and validate.
    #[test]
    fn draft_seven_schema_validates() {
        let schema = json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "type": "object",
            "properties": {"threads": {"type": "integer"}},
            "required": ["threads"]
        });
        let validator = SchemaValidator::new(&schema).unwrap();
        assert!(
            validator
                .validate(&json!({"threads": 4}), &AssumeExisting)
                .is_empty()
        );
        let issues = validator.validate(&json!({"threads": "x"}), &AssumeExisting);
        assert_eq!(issues[0].column.as_deref(), Some("threads"));
    }

    /// A schema that is not valid JSON Schema is rejected up front.
    #[test]
    fn invalid_schema_is_rejected() {
        let result = SchemaValidator::new(&json!({"type": 12}));
        assert!(matches!(result, Err(Error::InvalidSchema(_))));
    }
}
