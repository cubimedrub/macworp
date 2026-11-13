use serde_json::Value;

use crate::workflow_configuration::parameters::{
    Parameter,
    parameter::{IsParameterWithValue, Parameter as GenericParameter},
    path::Path as PathParameter,
    sample_sheet::SampleSheet,
    separator::Separator,
    text::Text,
    value_select::{SelectOption, ValueSelect},
};

/// Parameters never imported: set by the platform (output goes into the project)
const PLATFORM_PARAMETERS: &[&str] = &["outdir"];

/// Result of [`parameters_from_nextflow_schema`]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Imported {
    /// Generated parameters in schema order, parameter groups become separators
    pub parameters: Vec<Parameter>,
    /// `(name, reason)` of schema properties that were not imported
    pub skipped: Vec<(String, String)>,
}

/// Generates parameters from a parsed `nextflow_schema.json`.
///
/// Groups (`$defs` / `definitions`) become [`Separator`]s followed by their properties. Hidden
/// properties, platform controlled ones (`outdir`) and shapes that have no matching parameter
/// type are skipped and reported in [`Imported::skipped`]; they never fail the import.
/// The sample sheet property (`input` with a `schema` key) becomes a [`SampleSheet`] stub.
///
/// # Arguments
/// * `schema` - The parsed `nextflow_schema.json`
pub fn parameters_from_nextflow_schema(schema: &Value) -> Imported {
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
            imported
                .parameters
                .push(Parameter::Separator(Separator::new(
                    title.to_string(),
                    desc.to_string(),
                )));
            for (name, property) in properties {
                import_property(&mut imported, name, property);
            }
            if imported.parameters.len() == before + 1 {
                imported.parameters.pop();
            }
        }
    }

    // Top level properties outside of any group
    if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
        for (name, property) in properties {
            import_property(&mut imported, name, property);
        }
    }

    imported
}

/// Converts one schema property into a parameter and appends it, or records why it was skipped.
///
/// # Arguments
/// * `imported` - Result collecting parameters and skipped names
/// * `name` - Property name, used as the parameter name
/// * `property` - The property's JSON schema
fn import_property(imported: &mut Imported, name: &str, property: &Value) {
    let mut skip = |reason: &str| {
        imported
            .skipped
            .push((name.to_string(), reason.to_string()))
    };

    if PLATFORM_PARAMETERS.contains(&name) {
        return skip("set by the platform");
    }
    if property.get("hidden").and_then(Value::as_bool) == Some(true) {
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

    let parameter = if name == "input" && property.get("schema").is_some() {
        let mut sheet = SampleSheet::new(name.to_string(), label, desc);
        *sheet.value_mut() = default_string;
        Parameter::SampleSheet(sheet)
    } else {
        match property.get("type").and_then(Value::as_str) {
            Some("string") => {
                if let Some(values) = property.get("enum").and_then(Value::as_array) {
                    let mut select = ValueSelect::new(name.to_string(), label, desc);
                    *select.options_mut() = values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(|v| SelectOption {
                            label: v.to_string(),
                            value: v.to_string(),
                        })
                        .collect();
                    *select.value_mut() = default_string.map(|d| vec![d]);
                    Parameter::ValueSelect(select)
                } else if matches!(format, Some("file-path" | "directory-path" | "path")) {
                    let mut path = PathParameter::new(name.to_string(), label, desc);
                    if format == Some("directory-path") {
                        *path.is_excepting_files_mut() = true;
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
                let mut select = ValueSelect::new(name.to_string(), label, desc);
                *select.options_mut() = ["true", "false"]
                    .iter()
                    .map(|v| SelectOption {
                        label: v.to_string(),
                        value: v.to_string(),
                    })
                    .collect();
                *select.value_mut() = default
                    .and_then(Value::as_bool)
                    .map(|b| vec![b.to_string()]);
                Parameter::ValueSelect(select)
            }
            _ => return skip("unsupported type"),
        }
    };
    imported.parameters.push(parameter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::requests::ParameterValue;
    use crate::workflow_configuration::{
        WorkflowConfiguration,
        parameters::parameter::IsParameter,
        render::{apply_values, render_args},
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
                        "ref": {"type": "string", "format": "file-path"},
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

    /// Groups give separators, groups without visible properties give none.
    #[test]
    fn groups_become_separators_and_empty_groups_vanish() {
        let imported = parameters_from_nextflow_schema(&schema());
        let separators: Vec<_> = imported
            .parameters
            .iter()
            .filter_map(|p| match p {
                Parameter::Separator(s) => Some(s.label().to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(separators, ["Input/output options", "Options"]);
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
        assert!(matches!(find(&imported, "ref"), Parameter::Path(_)));
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
        let args = render_args(&config, None).unwrap();
        assert!(
            args.windows(2).any(|w| w == ["--input", "sheet.csv"]),
            "{args:?}"
        );
        assert!(args.windows(2).any(|w| w == ["--threads", "8"]), "{args:?}");
    }
}
