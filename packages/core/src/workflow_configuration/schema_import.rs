use serde_json::Value;

use crate::workflow_configuration::render::escape_path_segment;

use crate::workflow_configuration::parameters::{
    Parameter,
    group::Group,
    number::{Float, Integer},
    parameter::{IsParameterWithValue, Parameter as GenericParameter},
    path::Path as PathParameter,
    sample_sheet::SampleSheet,
    text::Text,
    value_select::{SelectOption, ValueSelect},
};

/// Parameters never imported: set by the platform (output goes into the project)
const PLATFORM_PARAMETERS: &[&str] = &["outdir"];

/// Result of [`parameters_from_nextflow_schema`]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Imported {
    /// Generated parameters in schema order, parameter groups become groups
    pub parameters: Vec<Parameter>,
    /// `(name, reason)` of schema properties that were not imported
    pub skipped: Vec<(String, String)>,
}

impl Imported {
    /// Appends the result of another schema. A parameter whose name is already present is dropped
    /// (the first definition wins); groups have no name and are always kept. Skipped entries are
    /// tagged with `source`.
    ///
    /// # Arguments
    /// * `other` - Result to append
    /// * `source` - Where `other` comes from (file path), appended to its skipped names
    pub fn merge(&mut self, other: Imported, source: &str) {
        for parameter in other.parameters {
            let duplicate = !parameter.name().is_empty()
                && self
                    .parameters
                    .iter()
                    .any(|existing| existing.name() == parameter.name());
            if !duplicate {
                self.parameters.push(parameter);
            }
        }
        self.skipped.extend(
            other
                .skipped
                .into_iter()
                .map(|(name, reason)| (format!("{name} ({source})"), reason)),
        );
    }
}

/// Generates parameters from a parsed `nextflow_schema.json`.
///
/// Groups (`$defs` / `definitions`) become [`Group`]s followed by their properties. Hidden
/// properties, platform controlled ones (`outdir`) and shapes that have no matching parameter
/// type are skipped and reported in [`Imported::skipped`]; they never fail the import.
/// The sample sheet property (`input` with a `schema` key) becomes a [`SampleSheet`] stub.
///
/// # Arguments
/// * `schema` - The parsed `nextflow_schema.json`
pub fn parameters_from_nextflow_schema(schema: &Value) -> Imported {
    parameters_from_json_schema(schema, true)
}

/// Generates parameters from the text of a Snakemake config schema (`*.schema.yaml`), a plain
/// JSON schema written as YAML. Top level properties become parameters; nested objects with
/// `properties` become a [`Group`] (titled by the dotted path) whose parameters are named by the
/// dotted path (`ref.species`) and rendered as a Python dictionary on the command line. Free-form
/// objects and arrays are skipped and reported. Text that is not valid YAML (or not a mapping)
/// imports nothing and is reported as one skipped entry.
///
/// # Arguments
/// * `text` - Content of the schema file
pub fn parameters_from_snakemake_schema(text: &str) -> Imported {
    match serde_norway::from_str::<Value>(text) {
        Ok(schema) if schema.is_object() => parameters_from_json_schema(&schema, false),
        Ok(_) => Imported {
            skipped: vec![("schema".to_string(), "not a mapping".to_string())],
            ..Imported::default()
        },
        Err(error) => Imported {
            skipped: vec![("schema".to_string(), format!("invalid YAML: {error}"))],
            ..Imported::default()
        },
    }
}

/// Generates parameters from the text of a CWL document (the workflow's main script) out of its
/// `inputs`, given as a map (`id: type`) or a list (`- id: ...`); a `$graph` document uses its
/// `main` process (else the first one). Optional types (`string?`, `[null, string]`) are unwrapped.
/// `string`, `int`/`long`, `float`/`double`, `boolean`, enums, `File` and `Directory` become
/// parameters; arrays, records, `Any`, unions of several types, ids with a dot (CWL input names
/// are flat) and unknown types are skipped and reported. Invalid YAML imports nothing.
///
/// # Arguments
/// * `text` - Content of the `.cwl` file
pub fn parameters_from_cwl(text: &str) -> Imported {
    let skipped = |reason: String| Imported {
        skipped: vec![("schema".to_string(), reason)],
        ..Imported::default()
    };
    let document = match serde_norway::from_str::<Value>(text) {
        Ok(document) if document.is_object() => document,
        Ok(_) => return skipped("not a mapping".to_string()),
        Err(error) => return skipped(format!("invalid YAML: {error}")),
    };
    let process = match document.get("$graph").and_then(Value::as_array) {
        Some(graph) => graph
            .iter()
            .find(|entry| {
                entry
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| id.trim_start_matches('#') == "main")
            })
            .or_else(|| graph.first()),
        None => Some(&document),
    };
    let Some(inputs) = process.and_then(|process| process.get("inputs")) else {
        return skipped("no inputs".to_string());
    };
    let entries: Vec<(String, &Value)> = match inputs {
        Value::Object(map) => map.iter().map(|(id, v)| (id.clone(), v)).collect(),
        Value::Array(list) => list
            .iter()
            .filter_map(|v| Some((v.get("id")?.as_str()?.to_string(), v)))
            .collect(),
        _ => return skipped("inputs is neither a map nor a list".to_string()),
    };
    let mut imported = Imported::default();
    for (id, definition) in entries {
        let name = cwl_local_name(&id);
        match cwl_parameter(name, definition) {
            Ok(parameter) => imported.parameters.push(parameter),
            Err(reason) => imported.skipped.push((name.to_string(), reason)),
        }
    }
    imported
}

/// The last segment of a CWL identifier (`#main/reads`, `file.cwl#reads` → `reads`).
///
/// # Arguments
/// * `id` - Identifier as written in the document
fn cwl_local_name(id: &str) -> &str {
    id.rsplit(['/', '#']).next().unwrap_or(id)
}

/// Converts one CWL input definition into a parameter, or the reason it cannot be one.
///
/// # Arguments
/// * `name` - Input name
/// * `definition` - Either the type alone (`string`, `[null, File]`, `{type: enum, ...}`) or an
///   object with `type`, `label`, `doc` and `default`
fn cwl_parameter(name: &str, definition: &Value) -> Result<Parameter, String> {
    if name.is_empty() || name.contains('.') {
        return Err("name not usable as a flat input name".to_string());
    }
    let fields = definition.as_object();
    let type_value = match fields {
        Some(fields) if is_type_object(fields) => definition,
        Some(fields) => fields.get("type").ok_or("no type")?,
        None => definition,
    };
    let label = fields
        .and_then(|f| f.get("label"))
        .and_then(Value::as_str)
        .map_or_else(|| name.to_string(), str::to_string);
    let desc = match fields.and_then(|f| f.get("doc")) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(lines)) => lines
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    };
    let default = fields.and_then(|f| f.get("default"));
    let default_string = default.and_then(|d| match d {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    });

    match cwl_type(type_value)? {
        CwlType::Name(kind) => match kind.as_str() {
            "string" => {
                let mut text = Text::new(name.to_string(), label, desc);
                *text.value_mut() = default_string;
                Ok(Parameter::Text(text))
            }
            "int" | "long" => {
                let mut integer = Integer::new(name.to_string(), label, desc);
                *integer.value_mut() = default.and_then(Value::as_i64);
                Ok(Parameter::Integer(integer))
            }
            "float" | "double" => {
                let mut float = Float::new(name.to_string(), label, desc);
                *float.value_mut() = default.and_then(Value::as_f64);
                Ok(Parameter::Float(float))
            }
            "boolean" => Ok(truth_choice(
                name,
                label,
                desc,
                default.and_then(Value::as_bool).map(|b| b.to_string()),
            )),
            "File" | "Directory" => {
                let mut path = PathParameter::new(name.to_string(), label, desc);
                *path.is_excepting_files_mut() = kind == "File";
                *path.is_excepting_folders_mut() = kind == "Directory";
                Ok(Parameter::Path(path))
            }
            other => Err(format!("unsupported type `{other}`")),
        },
        CwlType::Enum(symbols) => Ok(choice(
            name,
            label,
            desc,
            symbols,
            default_string.map(|d| cwl_local_name(&d).to_string()),
        )),
    }
}

/// Whether an object is a type description (`{type: enum|array|record, ...}`) rather than an input
/// definition that has a `type` field of its own.
///
/// # Arguments
/// * `fields` - The object
fn is_type_object(fields: &serde_json::Map<String, Value>) -> bool {
    matches!(
        fields.get("type").and_then(Value::as_str),
        Some("enum" | "array" | "record")
    ) && !fields.contains_key("label")
        && !fields.contains_key("doc")
        && !fields.contains_key("default")
        && !fields.contains_key("inputBinding")
        && !fields.contains_key("id")
}

/// A CWL type after unwrapping optionality.
enum CwlType {
    /// A primitive or class name (`string`, `File`, ...)
    Name(String),
    /// An enum with its symbols reduced to local names
    Enum(Vec<String>),
}

/// Reduces a CWL type to a single [`CwlType`]: `X?` and `[null, X]` become `X`, anything with
/// several non-null alternatives, arrays and records fail.
///
/// # Arguments
/// * `value` - The type as written in the document
fn cwl_type(value: &Value) -> Result<CwlType, String> {
    match value {
        Value::String(name) => {
            let name = name.strip_suffix('?').unwrap_or(name);
            if name.ends_with("[]") {
                return Err("array types are not supported".to_string());
            }
            Ok(CwlType::Name(name.to_string()))
        }
        Value::Array(alternatives) => {
            // YAML reads a bare `null` as a null value, JSON-style documents write the string
            let mut rest = alternatives
                .iter()
                .filter(|a| !a.is_null() && a.as_str() != Some("null"));
            match (rest.next(), rest.next()) {
                (Some(only), None) => cwl_type(only),
                (None, _) => Err("only null".to_string()),
                _ => Err("union of several types".to_string()),
            }
        }
        Value::Object(fields) => match fields.get("type").and_then(Value::as_str) {
            Some("enum") => Ok(CwlType::Enum(
                fields
                    .get("symbols")
                    .and_then(Value::as_array)
                    .map(|symbols| {
                        symbols
                            .iter()
                            .filter_map(Value::as_str)
                            .map(|s| cwl_local_name(s).to_string())
                            .collect()
                    })
                    .unwrap_or_default(),
            )),
            Some("array") => Err("array types are not supported".to_string()),
            Some("record") => Err("record types are not supported".to_string()),
            _ => Err("unknown type".to_string()),
        },
        _ => Err("unknown type".to_string()),
    }
}

/// Shared importer of Nextflow and Snakemake schemas: groups (`$defs` / `definitions`) become
/// [`Group`]s followed by their properties, then top level properties follow.
///
/// # Arguments
/// * `schema` - The parsed JSON schema
/// * `nextflow` - Apply the Nextflow conventions (`hidden`, platform parameters, sample sheet
///   `input`); false for engines where every property is an ordinary parameter
fn parameters_from_json_schema(schema: &Value, nextflow: bool) -> Imported {
    let mut imported = Imported::default();

    let groups = ["$defs", "definitions"]
        .iter()
        .find_map(|key| schema.get(key).and_then(Value::as_object));

    if let Some(groups) = groups {
        for group in groups.values() {
            let Some(properties) = group.get("properties").and_then(Value::as_object) else {
                continue;
            };
            let title = group
                .get("title")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let desc = group
                .get("description")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let before = imported.parameters.len();
            imported.parameters.push(Parameter::Group(Group::new(
                title.to_string(),
                desc.to_string(),
            )));
            for (name, property) in properties {
                import_property(&mut imported, name, property, nextflow);
            }
            if imported.parameters.len() == before + 1 {
                imported.parameters.pop();
            }
        }
    }

    // Top level properties outside of any group
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        if nextflow {
            for (name, property) in properties {
                import_property(&mut imported, name, property, nextflow);
            }
        } else {
            import_object(&mut imported, "", properties);
        }
    }

    imported
}

/// Imports the properties of an object schema of a non-Nextflow engine: plain properties first
/// (named `prefix.name`), then every nested object with `properties` as a [`Group`] titled by its
/// path followed by its own properties, recursively. Groups are flat headers, so plain
/// properties must precede the nested objects or they would show up inside the last group. An
/// object without `properties` (free-form) is skipped and reported.
///
/// # Arguments
/// * `imported` - Result collecting parameters and skipped names
/// * `prefix` - Dotted path of the object, empty for the schema root
/// * `properties` - The object's `properties`
fn import_object(
    imported: &mut Imported,
    prefix: &str,
    properties: &serde_json::Map<String, Value>,
) {
    let path = |name: &str| {
        let key = escape_path_segment(name);
        if prefix.is_empty() {
            key
        } else {
            format!("{prefix}.{key}")
        }
    };
    let mut nested = Vec::new();
    for (name, property) in properties {
        if schema_type(property) == Some("object") {
            match property.get("properties").and_then(Value::as_object) {
                Some(children) => nested.push((name, property, children)),
                None => imported
                    .skipped
                    .push((path(name), "object without properties".to_string())),
            }
        } else {
            import_property(imported, &path(name), property, false);
        }
    }
    for (name, property, children) in nested {
        let full = path(name);
        let title = property
            .get("title")
            .and_then(Value::as_str)
            .map_or_else(|| full.clone(), str::to_string);
        let desc = property
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let before = imported.parameters.len();
        imported
            .parameters
            .push(Parameter::Group(Group::new(title, desc.to_string())));
        let header = imported.parameters.len();
        import_object(imported, &full, children);
        // Plain properties come first, so a header directly followed by another group (or by
        // nothing) carries no parameters of its own
        if imported.parameters[header..]
            .first()
            .is_none_or(|p| matches!(p, Parameter::Group(_)))
        {
            imported.parameters.remove(before);
        }
    }
}

/// The single type of a property: `type` itself, or for a list of types (`[array, string]`) the
/// first one that is not `null`, preferring `string` as the most permissive scalar.
///
/// # Arguments
/// * `property` - The property's JSON schema
fn schema_type(property: &Value) -> Option<&str> {
    match property.get("type")? {
        Value::String(single) => Some(single),
        Value::Array(types) => {
            let names: Vec<&str> = types.iter().filter_map(Value::as_str).collect();
            if names.contains(&"string") {
                Some("string")
            } else {
                names.into_iter().find(|name| *name != "null")
            }
        }
        _ => None,
    }
}

/// Converts one schema property into a parameter and appends it, or records why it was skipped.
///
/// # Arguments
/// * `imported` - Result collecting parameters and skipped names
/// * `name` - Property name, used as the parameter name
/// * `property` - The property's JSON schema
/// * `nextflow` - Apply the Nextflow conventions, see [`parameters_from_json_schema`]
fn import_property(imported: &mut Imported, name: &str, property: &Value, nextflow: bool) {
    let mut skip = |reason: &str| {
        imported
            .skipped
            .push((name.to_string(), reason.to_string()))
    };

    if nextflow && PLATFORM_PARAMETERS.contains(&name) {
        return skip("set by the platform");
    }
    if nextflow && property.get("hidden").and_then(Value::as_bool) == Some(true) {
        return skip("hidden");
    }

    let label = property
        .get("title")
        .and_then(Value::as_str)
        .map_or_else(|| name.to_string(), str::to_string);
    let desc = property
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let format = property.get("format").and_then(Value::as_str);
    let default = property.get("default");
    let default_string = default.map(|d| match d {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    });

    let parameter = if nextflow && name == "input" && property.get("schema").is_some() {
        let mut sheet = SampleSheet::new(name.to_string(), label, desc);
        *sheet.value_mut() = default_string;
        Parameter::SampleSheet(sheet)
    } else {
        match schema_type(property) {
            Some("string") => {
                if let Some(values) = property.get("enum").and_then(Value::as_array) {
                    let options: Vec<String> = values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_string)
                        .collect();
                    choice(name, label, desc, options, default_string)
                } else if matches!(format, Some("file-path" | "directory-path" | "path")) {
                    let mut path = PathParameter::new(name.to_string(), label, desc);
                    // `file-path` selects files, `directory-path` folders, `path` either
                    *path.is_excepting_files_mut() = format != Some("directory-path");
                    *path.is_excepting_folders_mut() = format != Some("file-path");
                    if let Some(pattern) = property.get("pattern").and_then(Value::as_str) {
                        if format == Some("directory-path") {
                            path.set_folder_pattern(pattern);
                        } else {
                            path.set_file_pattern(pattern);
                        }
                    }
                    if let Some(default) = default_string {
                        path.value_from_string(default);
                    }
                    Parameter::Path(path)
                } else if format == Some("file-path-pattern") {
                    let mut glob = GenericParameter::<String>::new(name.to_string(), label, desc);
                    *glob.value_mut() = default_string;
                    Parameter::Glob(glob)
                } else {
                    let mut text = Text::new(name.to_string(), label, desc);
                    *text.value_mut() = default_string;
                    Parameter::Text(text)
                }
            }
            Some("integer") => {
                let mut integer = crate::workflow_configuration::parameters::number::Integer::new(
                    name.to_string(),
                    label,
                    desc,
                );
                *integer.min_mut() = property.get("minimum").and_then(Value::as_i64);
                *integer.max_mut() = property.get("maximum").and_then(Value::as_i64);
                *integer.value_mut() = default.and_then(Value::as_i64);
                Parameter::Integer(integer)
            }
            Some("number") => {
                let mut float = crate::workflow_configuration::parameters::number::Float::new(
                    name.to_string(),
                    label,
                    desc,
                );
                *float.min_mut() = property.get("minimum").and_then(Value::as_f64);
                *float.max_mut() = property.get("maximum").and_then(Value::as_f64);
                *float.value_mut() = default.and_then(Value::as_f64);
                Parameter::Float(float)
            }
            // No boolean parameter type: a true/false choice
            Some("boolean") => {
                let default = default.and_then(Value::as_bool).map(|b| b.to_string());
                truth_choice(name, label, desc, default)
            }
            _ => return skip("unsupported type"),
        }
    };
    imported.parameters.push(parameter);
}

/// A [`ValueSelect`] offering `options` (label = value).
///
/// # Arguments
/// * `name` - Parameter name
/// * `label` - Parameter label
/// * `desc` - Parameter description
/// * `options` - Selectable values
/// * `default` - Preselected value
fn choice(
    name: &str,
    label: String,
    desc: String,
    options: Vec<String>,
    default: Option<String>,
) -> Parameter {
    let mut select = ValueSelect::new(name.to_string(), label, desc);
    *select.options_mut() = options
        .into_iter()
        .map(|value| SelectOption {
            label: value.clone(),
            value,
        })
        .collect();
    *select.value_mut() = default.map(|d| vec![d]);
    Parameter::ValueSelect(select)
}

/// A true/false [`ValueSelect`] (there is no boolean parameter type).
///
/// # Arguments
/// * `name` - Parameter name
/// * `label` - Parameter label
/// * `desc` - Parameter description
/// * `default` - Preselected `"true"` or `"false"`
fn truth_choice(name: &str, label: String, desc: String, default: Option<String>) -> Parameter {
    choice(
        name,
        label,
        desc,
        vec!["true".to_string(), "false".to_string()],
        default,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::requests::ParameterValue;
    use crate::workflow_configuration::{
        WorkflowConfiguration,
        parameters::parameter::IsParameter,
        render::{apply_values, render_parameter_file},
    };
    use serde_json::json;

    /// A small `nextflow_schema.json` covering every supported type and the skipped cases.
    fn schema() -> Value {
        json!({
            "$defs": {
                "input_output_options": {
                    "title": "Input/output options",
                    "description": "Where data comes from",
                    "properties": {
                        "input": {"type": "string", "format": "file-path", "schema": "assets/schema_input.json", "description": "sheet"},
                        "outdir": {"type": "string", "format": "directory-path"},
                        "email": {"type": "string", "hidden": true}
                    }
                },
                "empty_group": {
                    "title": "Empty",
                    "properties": {"secret": {"type": "string", "hidden": true}}
                },
                "options": {
                    "title": "Options",
                    "properties": {
                        "genome": {"type": "string", "enum": ["GRCh37", "GRCh38"], "default": "GRCh38"},
                        "ref": {"type": "string", "format": "file-path", "pattern": ".*\\.fa"},
                        "dir": {"type": "string", "format": "directory-path", "default": "data"},
                        "pattern": {"type": "string", "format": "file-path-pattern"},
                        "label": {"type": "string", "default": "x"},
                        "threads": {"type": "integer", "minimum": 1, "maximum": 64, "default": 4},
                        "ratio": {"type": "number", "minimum": 0.0, "maximum": 1.0},
                        "skip_qc": {"type": "boolean", "default": false},
                        "weird": {"type": "array"}
                    }
                }
            }
        })
    }

    /// Looks up an imported parameter by name, panicking if absent.
    fn find<'a>(imported: &'a Imported, name: &str) -> &'a Parameter {
        imported
            .parameters
            .iter()
            .find(|p| p.name() == name)
            .unwrap_or_else(|| panic!("`{name}` not imported"))
    }

    /// Groups give groups, groups without visible properties give none.
    #[test]
    fn groups_are_headed_and_empty_groups_vanish() {
        let imported = parameters_from_nextflow_schema(&schema());
        let groups: Vec<_> = imported
            .parameters
            .iter()
            .filter_map(|p| match p {
                Parameter::Group(s) => Some(s.label().to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(groups, ["Input/output options", "Options"]);
    }

    /// Each schema type maps to its parameter type, ranges are kept.
    #[test]
    fn types_are_mapped() {
        let imported = parameters_from_nextflow_schema(&schema());
        assert!(matches!(
            find(&imported, "input"),
            Parameter::SampleSheet(_)
        ));
        assert!(matches!(
            find(&imported, "genome"),
            Parameter::ValueSelect(_)
        ));
        let Parameter::Path(reference) = find(&imported, "ref") else {
            panic!("ref is not a path");
        };
        assert_eq!(reference.file_pattern(), Some(r".*\.fa"));
        assert!(reference.is_excepting_files() && !reference.is_excepting_folders());
        let Parameter::Path(directory) = find(&imported, "dir") else {
            panic!("dir is not a path");
        };
        assert!(!directory.is_excepting_files() && directory.is_excepting_folders());
        assert!(matches!(find(&imported, "pattern"), Parameter::Glob(_)));
        assert!(matches!(find(&imported, "label"), Parameter::Text(_)));
        assert!(matches!(find(&imported, "threads"), Parameter::Integer(_)));
        assert!(matches!(find(&imported, "ratio"), Parameter::Float(_)));
        assert!(matches!(
            find(&imported, "skip_qc"),
            Parameter::ValueSelect(_)
        ));
        let Parameter::Integer(threads) = find(&imported, "threads") else {
            unreachable!()
        };
        assert_eq!((threads.min(), threads.max()), (Some(1), Some(64)));
    }

    /// Hidden, platform controlled and unsupported properties are reported as skipped.
    #[test]
    fn skipped_are_reported() {
        let imported = parameters_from_nextflow_schema(&schema());
        let names: Vec<_> = imported.skipped.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"outdir"));
        assert!(names.contains(&"email"));
        assert!(names.contains(&"weird"));
        assert!(imported.parameters.iter().all(|p| p.name() != "outdir"));
    }

    /// Malformed schemas import nothing and do not panic.
    #[test]
    fn garbage_does_not_panic() {
        assert_eq!(
            parameters_from_nextflow_schema(&json!(null)),
            Imported::default()
        );
        assert_eq!(
            parameters_from_nextflow_schema(&json!({"$defs": {"a": 3}, "properties": 5})),
            Imported::default()
        );
    }

    /// Imported parameters accept values and render as command line arguments.
    #[test]
    fn imported_parameters_render() {
        let imported = parameters_from_nextflow_schema(&schema());
        let mut config = WorkflowConfiguration::default();
        *config.dynamic_parameters_mut() = imported.parameters;
        let values = [
            ("input", json!("sheet.csv")),
            ("genome", json!(["GRCh37"])),
            ("threads", json!(8)),
        ]
        .map(|(name, value)| ParameterValue {
            name: name.to_string(),
            value,
        });
        apply_values(&mut config, &values).unwrap();
        let rendered =
            render_parameter_file(&config, None, None, crate::paths::PathMapping::Native)
                .unwrap()
                .unwrap();
        assert_eq!(rendered["input"], json!("sheet.csv"));
        assert_eq!(rendered["threads"], json!(8));
    }

    /// A Snakemake config schema as found in the wild (YAML, 2020-12 dialect).
    const SNAKEMAKE_SCHEMA: &str = r#"
$schema: "https://json-schema.org/draft/2020-12/schema"
type: object
properties:
  genome:
    type: string
    enum: [GRCh37, GRCh38]
    default: GRCh38
  threads:
    type: integer
    minimum: 1
    default: 4
  ratio: {type: number, maximum: 1.0}
  skip_qc: {type: boolean, default: false}
  label: {type: string, description: "free text"}
  input:
    type: string
    schema: sheet.json
  outdir: {type: string, hidden: true}
  samples: {type: array}
  free: {type: object}
  "dotted.key": {type: string}
  ref:
    type: object
    properties:
      species: {type: string}
      release: {type: integer}
  params:
    type: object
    properties:
      star:
        type: object
        properties:
          index: {type: string}
  pca:
    type: object
    properties:
      labels: {type: [array, string], items: {type: string}}
required: [genome]
"#;

    /// Snakemake import maps types and treats Nextflow conventions as ordinary properties.
    #[test]
    fn snakemake_schema_is_imported() {
        let imported = parameters_from_snakemake_schema(SNAKEMAKE_SCHEMA);
        assert!(matches!(
            find(&imported, "genome"),
            Parameter::ValueSelect(_)
        ));
        assert!(matches!(find(&imported, "threads"), Parameter::Integer(_)));
        assert!(matches!(find(&imported, "ratio"), Parameter::Float(_)));
        assert!(matches!(
            find(&imported, "skip_qc"),
            Parameter::ValueSelect(_)
        ));
        assert!(matches!(find(&imported, "label"), Parameter::Text(_)));
        // `input` / `outdir` / `hidden` carry no Nextflow meaning here
        assert!(matches!(find(&imported, "input"), Parameter::Text(_)));
        assert!(matches!(find(&imported, "outdir"), Parameter::Text(_)));
        let skipped: Vec<_> = imported.skipped.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(skipped, ["samples", "free"]);
        // a dot inside a key is escaped so it is not read as a nested path
        assert!(matches!(
            find(&imported, r"dotted\.key"),
            Parameter::Text(_)
        ));
    }

    /// Nested objects become groups titled by their path with dotted parameter names; plain
    /// properties come first, a header without own parameters vanishes, type lists are read.
    #[test]
    fn snakemake_nested_objects_become_groups() {
        let imported = parameters_from_snakemake_schema(SNAKEMAKE_SCHEMA);
        let order: Vec<String> = imported
            .parameters
            .iter()
            .map(|p| match p {
                Parameter::Group(g) => format!("[{}]", g.label()),
                other => other.name().to_string(),
            })
            .collect();
        let tail: Vec<&str> = order
            .iter()
            .skip_while(|n| *n != "[ref]")
            .map(String::as_str)
            .collect();
        assert_eq!(
            tail,
            [
                "[ref]",
                "ref.species",
                "ref.release",
                "[params.star]",
                "params.star.index",
                "[pca]",
                "pca.labels"
            ]
        );
        assert!(matches!(
            find(&imported, "ref.release"),
            Parameter::Integer(_)
        ));
        assert!(matches!(find(&imported, "pca.labels"), Parameter::Text(_)));
        assert!(!order.contains(&"[params]".to_string()));
    }

    /// Merging keeps every group (they are unnamed) and drops repeated parameter names.
    #[test]
    fn merge_keeps_groups_and_drops_duplicates() {
        let mut merged = Imported::default();
        merged.merge(parameters_from_snakemake_schema(SNAKEMAKE_SCHEMA), "a.yaml");
        let groups = |imported: &Imported| {
            imported
                .parameters
                .iter()
                .filter(|p| matches!(p, Parameter::Group(_)))
                .count()
        };
        let once = (merged.parameters.len(), groups(&merged));
        assert_eq!(groups(&merged), 3);
        merged.merge(parameters_from_snakemake_schema(SNAKEMAKE_SCHEMA), "b.yaml");
        // same names again: only the three groups are added
        assert_eq!(merged.parameters.len(), once.0 + 3);
        assert!(merged.skipped.iter().any(|(n, _)| n == "samples (b.yaml)"));
    }

    /// Invalid YAML and non-mappings import nothing, report one entry and do not panic.
    #[test]
    fn snakemake_garbage_does_not_panic() {
        for text in ["", "- a\n- b", "{{{", "just text"] {
            let imported = parameters_from_snakemake_schema(text);
            assert!(imported.parameters.is_empty(), "{text:?}");
            assert_eq!(imported.skipped.len(), 1, "{text:?}");
        }
    }

    /// Imported Snakemake parameters render into the configuration file.
    #[test]
    fn snakemake_parameters_render() {
        let imported = parameters_from_snakemake_schema(SNAKEMAKE_SCHEMA);
        let mut config = WorkflowConfiguration::default();
        *config.engine_mut() = crate::workflow_configuration::engine::Engine::Snakemake;
        *config.dynamic_parameters_mut() = imported.parameters;
        let values = [("genome", json!(["GRCh37"])), ("threads", json!(8))].map(|(name, value)| {
            ParameterValue {
                name: name.to_string(),
                value,
            }
        });
        apply_values(&mut config, &values).unwrap();
        let rendered =
            render_parameter_file(&config, None, None, crate::paths::PathMapping::Native)
                .unwrap()
                .unwrap();
        assert_eq!(rendered["genome"], json!("GRCh37"));
        assert_eq!(rendered["threads"], json!(8));
    }

    /// A CWL document written with an input map covering every supported type.
    const CWL_MAP: &str = r#"
cwlVersion: v1.2
class: Workflow
inputs:
  reads:
    type: File
    label: Reads
    doc: [first line, second line]
  folder: Directory?
  name: {type: string, default: sample}
  threads: {type: [null, int], default: 4}
  ratio: double
  verbose: {type: boolean, default: true}
  mode:
    type:
      type: enum
      symbols: ["file.cwl#mode/fast", "file.cwl#mode/slow"]
    default: fast
  many: File[]
  anything: Any
  pair: [string, int]
  rec: {type: record, fields: []}
  dotted.name: string
outputs: {}
steps: {}
"#;

    /// Each CWL type maps to its parameter type; optional types are unwrapped.
    #[test]
    fn cwl_types_are_mapped() {
        let imported = parameters_from_cwl(CWL_MAP);
        match find(&imported, "reads") {
            Parameter::Path(p) => {
                assert!(p.is_excepting_files());
                assert!(!p.is_excepting_folders());
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(find(&imported, "reads").label(), "Reads");
        match find(&imported, "folder") {
            Parameter::Path(p) => {
                assert!(!p.is_excepting_files());
                assert!(p.is_excepting_folders());
            }
            other => panic!("{other:?}"),
        }
        assert!(
            matches!(find(&imported, "name"), Parameter::Text(t) if t.value() == Some(&"sample".to_string()))
        );
        assert!(
            matches!(find(&imported, "threads"), Parameter::Integer(i) if i.value() == Some(&4))
        );
        assert!(matches!(find(&imported, "ratio"), Parameter::Float(_)));
        match find(&imported, "verbose") {
            Parameter::ValueSelect(s) => {
                assert_eq!(s.value(), Some(&vec!["true".to_string()]));
                assert_eq!(s.options().len(), 2);
            }
            other => panic!("{other:?}"),
        }
        match find(&imported, "mode") {
            Parameter::ValueSelect(s) => {
                let values: Vec<_> = s.options().iter().map(|o| o.value.as_str()).collect();
                assert_eq!(values, ["fast", "slow"]);
                assert_eq!(s.value(), Some(&vec!["fast".to_string()]));
            }
            other => panic!("{other:?}"),
        }
    }

    /// Arrays, records, `Any`, unions and dotted names are reported, never imported.
    #[test]
    fn cwl_unsupported_inputs_are_skipped() {
        let imported = parameters_from_cwl(CWL_MAP);
        let skipped: Vec<_> = imported.skipped.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(skipped, ["many", "anything", "pair", "rec", "dotted.name"]);
        assert_eq!(imported.parameters.len(), 7);
    }

    /// The list form with `#`-prefixed ids and a `$graph` document with a `main` process.
    #[test]
    fn cwl_list_and_graph_forms() {
        let list = "inputs:\n  - id: '#main/reads'\n    type: File\n  - id: n\n    type: int\n";
        let imported = parameters_from_cwl(list);
        let names: Vec<_> = imported.parameters.iter().map(|p| p.name()).collect();
        assert_eq!(names, ["reads", "n"]);

        let graph = r#"
$graph:
  - id: tool
    inputs: {other: string}
  - id: '#main'
    inputs: {wanted: string}
"#;
        let imported = parameters_from_cwl(graph);
        assert_eq!(imported.parameters.len(), 1);
        assert_eq!(imported.parameters[0].name(), "wanted");
    }

    /// Invalid YAML, non-mappings and documents without inputs import nothing and do not panic.
    #[test]
    fn cwl_garbage_does_not_panic() {
        for text in ["", "- a", "{{{", "class: Workflow", "inputs: 3"] {
            let imported = parameters_from_cwl(text);
            assert!(imported.parameters.is_empty(), "{text:?}");
            assert_eq!(imported.skipped.len(), 1, "{text:?}");
        }
    }

    /// Imported CWL parameters render into the input object.
    // Native mapping with POSIX host paths: a Windows host uses a mapped style (see the `PathMapping` tests).
    #[cfg(unix)]
    #[test]
    fn cwl_parameters_render() {
        let imported = parameters_from_cwl(CWL_MAP);
        let mut config = WorkflowConfiguration::default();
        *config.engine_mut() = crate::workflow_configuration::engine::Engine::Cwl;
        *config.dynamic_parameters_mut() = imported.parameters;
        let values = [
            ("reads", json!(["a.fq"])),
            ("threads", json!(8)),
            ("verbose", json!(["false"])),
        ]
        .map(|(name, value)| ParameterValue {
            name: name.to_string(),
            value,
        });
        apply_values(&mut config, &values).unwrap();
        let rendered = render_parameter_file(
            &config,
            None,
            Some(std::path::Path::new("/p")),
            crate::paths::PathMapping::Native,
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            rendered["reads"],
            json!({"class": "File", "path": "/p/a.fq"})
        );
        assert_eq!(rendered["threads"], json!(8));
        assert_eq!(rendered["verbose"], json!(false));
        assert_eq!(rendered["mode"], json!("fast"));
    }
}
