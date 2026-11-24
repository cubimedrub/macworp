use std::path::{Path, PathBuf};

use serde_json::Value;
use thiserror::Error;

use super::{
    WorkflowConfiguration,
    engine::Engine,
    parameters::{Parameter, parameter::IsParameterWithValue},
};
use crate::{
    paths::{PathMapping, is_confined, sanitize_relative, strip_reference_prefix},
    requests::ParameterValue,
};

/// Why parameter values could not be applied or a configuration could not be rendered
#[derive(Debug, Error, PartialEq)]
pub enum Error {
    /// No dynamic parameter with this name
    #[error("Unknown parameter `{0}`")]
    UnknownParameter(String),
    /// The JSON value does not fit the parameter type (parameter name, expected type)
    #[error("Value of parameter `{0}` has the wrong type, expected {1}")]
    TypeMismatch(String, &'static str),
    /// A value for a parameter that takes none (group)
    #[error("Parameter `{0}` takes no value")]
    NoValueAllowed(String),
    /// Path points out of the project directory (`..`) or is empty (parameter name)
    #[error("Path in parameter `{0}` must stay inside the project directory")]
    InvalidPath(String),
    /// Number outside `min`/`max` or more paths than `limit` (parameter name, reason)
    #[error("Value of parameter `{0}` is out of range: {1}")]
    OutOfRange(String, String),
    /// A path matches neither the file nor the folder pattern of its `Path` parameter (parameter
    /// name, path)
    #[error("Path `{1}` in parameter `{0}` does not match the allowed patterns")]
    PatternMismatch(String, String),
    /// Name is empty, starts with `-` or has characters other than letters, digits, `_`, `.`, `-`
    #[error("Invalid parameter name `{0}`, use letters, digits, `_`, `.` and `-` (not leading)")]
    InvalidName(String),
    /// The engine takes no engine parameters (Container)
    #[error("Engine `{0}` does not support engine parameters")]
    EngineParametersUnsupported(String),
    /// An engine parameter is not on the engine's allowlist or has a bad value
    #[error(transparent)]
    EngineParameter(#[from] super::engine_parameters::Error),
    /// The image builds of the workflow are invalid
    #[error(transparent)]
    Images(#[from] super::images::Error),
    /// A `Path` value points into the global reference data but the executor has no reference
    /// directory (parameter name)
    #[error("Parameter `{0}` uses the global reference data, which is not available here")]
    ReferenceUnavailable(String),
}

/// Names end up in command line flags, so they must not be able to form another flag or contain
/// groups. A backslash is only valid as the escape of `.` or `\` (see [`split_path`]).
fn validate_name(name: &str) -> Result<(), Error> {
    let valid = !name.is_empty()
        && !name.starts_with('-')
        && split_path(name).is_some()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-' | '\\'));
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidName(name.to_string()))
    }
}

/// Checks `min <= value <= max`, unset bounds are ignored
fn check_range<T: PartialOrd + std::fmt::Display + Copy>(
    name: &str,
    value: T,
    min: Option<T>,
    max: Option<T>,
) -> Result<(), Error> {
    if let Some(min) = min
        && value < min
    {
        return Err(Error::OutOfRange(
            name.to_string(),
            format!("{value} < minimum {min}"),
        ));
    }
    if let Some(max) = max
        && value > max
    {
        return Err(Error::OutOfRange(
            name.to_string(),
            format!("{value} > maximum {max}"),
        ));
    }
    Ok(())
}

/// Validates and normalises a user supplied path with the shared [`crate::paths`] rules.
fn project_path(name: &str, raw: &str) -> Result<PathBuf, Error> {
    let raw = std::path::Path::new(raw);
    let path = sanitize_relative(raw);
    if !is_confined(raw) || path.as_os_str().is_empty() {
        return Err(Error::InvalidPath(name.to_string()));
    }
    Ok(path)
}

/// Sets the value of one parameter from its JSON representation, `null` unsets it.
fn set_value(parameter: &mut Parameter, value: &Value) -> Result<(), Error> {
    let name = parameter.name().to_string();
    match parameter {
        Parameter::Text(p) => {
            *p.value_mut() = match value {
                Value::Null => None,
                Value::String(s) => Some(s.clone()),
                _ => return Err(Error::TypeMismatch(name, "string")),
            };
        }
        Parameter::SampleSheet(p) => {
            *p.value_mut() = match value {
                Value::Null => None,
                Value::String(s) => Some(s.clone()),
                _ => return Err(Error::TypeMismatch(name, "string")),
            };
        }
        Parameter::Glob(p) => {
            *p.value_mut() = match value {
                Value::Null => None,
                Value::String(s) => Some(s.clone()),
                _ => return Err(Error::TypeMismatch(name, "string")),
            };
        }
        Parameter::Integer(p) => {
            *p.value_mut() = match value {
                Value::Null => None,
                v => {
                    let v = v
                        .as_i64()
                        .ok_or_else(|| Error::TypeMismatch(name.clone(), "integer"))?;
                    check_range(&name, v, p.min(), p.max())?;
                    Some(v)
                }
            };
        }
        Parameter::Float(p) => {
            *p.value_mut() = match value {
                Value::Null => None,
                v => {
                    let v = v
                        .as_f64()
                        .filter(|v| v.is_finite())
                        .ok_or_else(|| Error::TypeMismatch(name.clone(), "number"))?;
                    check_range(&name, v, p.min(), p.max())?;
                    Some(v)
                }
            };
        }
        Parameter::Path(p) => {
            let paths: Option<Vec<PathBuf>> = match value {
                Value::Null => None,
                Value::String(s) => Some(vec![project_path(&name, s)?]),
                Value::Array(items) => Some(
                    items
                        .iter()
                        .map(|i| {
                            let s = i.as_str().ok_or_else(|| {
                                Error::TypeMismatch(name.clone(), "array of strings")
                            })?;
                            project_path(&name, s)
                        })
                        .collect::<Result<_, _>>()?,
                ),
                _ => return Err(Error::TypeMismatch(name, "array of strings")),
            };
            if let Some(paths) = &paths
                && paths.len() > p.limit().get()
            {
                return Err(Error::OutOfRange(
                    name,
                    format!("{} paths, at most {} allowed", paths.len(), p.limit()),
                ));
            }
            if let Some(rejected) = paths.iter().flatten().find(|path| !p.accepts_path(path)) {
                return Err(Error::PatternMismatch(name, rejected.display().to_string()));
            }
            *p.value_mut() = paths;
        }
        Parameter::ValueSelect(p) => {
            let values: Option<Vec<String>> = match value {
                Value::Null => None,
                Value::String(s) => Some(vec![s.clone()]),
                Value::Array(items) => Some(
                    items
                        .iter()
                        .map(|i| {
                            i.as_str().map(str::to_string).ok_or_else(|| {
                                Error::TypeMismatch(name.clone(), "array of strings")
                            })
                        })
                        .collect::<Result<_, _>>()?,
                ),
                _ => return Err(Error::TypeMismatch(name, "array of strings")),
            };
            if let Some(values) = &values {
                if values.len() > 1 && !p.is_multiselect() {
                    return Err(Error::OutOfRange(
                        name,
                        format!("{} values, only one allowed", values.len()),
                    ));
                }
                if let Some(bad) = values
                    .iter()
                    .find(|v| !p.options().iter().any(|o| &o.value == *v))
                {
                    return Err(Error::OutOfRange(
                        name,
                        format!("`{bad}` is not one of the options"),
                    ));
                }
            }
            *p.value_mut() = values;
        }
        Parameter::Group(_) => {
            if !value.is_null() {
                return Err(Error::NoValueAllowed(name));
            }
        }
    }
    Ok(())
}

/// JSON representation of a parameter's current value, the inverse of what [`apply_values`] accepts.
/// `null` if unset.
pub fn value_json(parameter: &Parameter) -> Value {
    match parameter {
        Parameter::Text(p) => p.value().map_or(Value::Null, |v| Value::from(v.as_str())),
        Parameter::Glob(p) => p.value().map_or(Value::Null, |v| Value::from(v.as_str())),
        Parameter::SampleSheet(p) => p.value().map_or(Value::Null, |v| Value::from(v.as_str())),
        Parameter::Integer(p) => p.value().map_or(Value::Null, |v| Value::from(*v)),
        Parameter::Float(p) => p.value().map_or(Value::Null, |v| Value::from(*v)),
        Parameter::Path(p) => p.value().map_or(Value::Null, |paths| {
            paths
                .iter()
                .map(|p| Value::from(p.display().to_string()))
                .collect()
        }),
        Parameter::ValueSelect(p) => p.value().map_or(Value::Null, |values| {
            values.iter().map(|v| Value::from(v.as_str())).collect()
        }),
        Parameter::Group(_) => Value::Null,
    }
}

/// Sets user supplied values on the dynamic parameters of `config`, matched by name.
///
/// Fails on unknown names and on values of the wrong type or outside the allowed range.
/// Nothing but dynamic parameter values is touched.
pub fn apply_values(
    config: &mut WorkflowConfiguration,
    values: &[ParameterValue],
) -> Result<(), Error> {
    for v in values {
        let parameter = config
            .dynamic_parameters_mut()
            .iter_mut()
            .find(|p| p.name() == v.name)
            .ok_or_else(|| Error::UnknownParameter(v.name.clone()))?;
        set_value(parameter, &v.value)?;
    }
    Ok(())
}

/// Like [`apply_values`] for values that may predate the current definition (e.g. a previous
/// run): values for unknown names, groups, or of a type or range that no longer fits are
/// skipped instead of failing. Returns the names that were skipped.
pub fn apply_values_lenient(
    config: &mut WorkflowConfiguration,
    values: &[ParameterValue],
) -> Vec<String> {
    let mut skipped = Vec::new();
    for v in values {
        let applied = config
            .dynamic_parameters_mut()
            .iter_mut()
            .find(|p| p.name() == v.name)
            .is_some_and(|p| set_value(p, &v.value).is_ok());
        if !applied {
            skipped.push(v.name.clone());
        }
    }
    skipped
}

/// A value of a `Path` parameter on its way to the command line.
struct ResolvedPath {
    /// The path on this machine: the value itself (relative to the project), or for a value below
    /// the reference prefix the path inside the reference directory
    host: std::path::PathBuf,
    /// Whether `host` is absolute because the value pointed into the reference data
    from_reference: bool,
}

impl ResolvedPath {
    /// The path as the engine sees it: reference paths in the container-side form, relative
    /// values as they are.
    ///
    /// # Arguments
    /// * `mapping` - How host paths look inside the container
    fn text(&self, mapping: PathMapping) -> String {
        if self.from_reference {
            mapping.container(&self.host)
        } else {
            self.host.display().to_string()
        }
    }
}

/// The values of a `Path` parameter, those below [`crate::paths::REFERENCE_PREFIX`] rewritten
/// onto the reference directory.
///
/// # Arguments
/// * `parameter` - The parameter the paths belong to, for the error
/// * `paths` - The values of the parameter
/// * `reference_directory` - Absolute directory of the global reference data, without it a
///   reference path is an error
fn resolve_paths(
    parameter: &Parameter,
    paths: &[std::path::PathBuf],
    reference_directory: Option<&Path>,
) -> Result<Vec<ResolvedPath>, Error> {
    let mut resolved = Vec::with_capacity(paths.len());
    for path in paths {
        resolved.push(match strip_reference_prefix(path) {
            Some(rest) => ResolvedPath {
                host: reference_directory
                    .ok_or_else(|| Error::ReferenceUnavailable(parameter.name().to_string()))?
                    .join(rest),
                from_reference: true,
            },
            None => ResolvedPath {
                host: path.clone(),
                from_reference: false,
            },
        });
    }
    Ok(resolved)
}

/// Renders the value of a parameter as it is passed on the command line, `None` if unset.
///
/// # Arguments
/// * `parameter` - The parameter to render
/// * `reference_directory` - Absolute directory of the global reference data. `Path` values below
///   [`crate::paths::REFERENCE_PREFIX`] are rewritten onto it; without it they are an error
/// * `mapping` - How host paths look inside the container, applied to the reference directory
fn value_string(
    parameter: &Parameter,
    reference_directory: Option<&Path>,
    mapping: PathMapping,
) -> Result<Option<String>, Error> {
    Ok(match parameter {
        Parameter::Text(p) => p.value().cloned(),
        Parameter::Glob(p) => p.value().cloned(),
        Parameter::SampleSheet(p) => p.value().cloned(),
        Parameter::Integer(p) => p.value().map(|v| v.to_string()),
        Parameter::Float(p) => p.value().map(|v| v.to_string()),
        Parameter::Path(p) => match p.value() {
            None => None,
            Some(paths) => Some(
                resolve_paths(parameter, paths, reference_directory)?
                    .iter()
                    .map(|resolved| resolved.text(mapping))
                    .collect::<Vec<_>>()
                    .join(","),
            ),
        },
        Parameter::ValueSelect(p) => p.value().map(|values| values.join(",")),
        Parameter::Group(_) => None,
    })
}

/// Escapes a key for use as one segment of a dotted parameter name: `\` becomes `\\` and `.`
/// becomes `\.`, so a key that contains a dot is not read as a nested path.
///
/// # Arguments
/// * `key` - A key of the workflow's configuration, as written in its schema
pub fn escape_path_segment(key: &str) -> String {
    key.replace('\\', "\\\\").replace('.', "\\.")
}

/// Splits a dotted parameter name into its unescaped keys: an unescaped `.` separates levels,
/// `\.` and `\\` stand for a literal dot and backslash. `None` for a dangling or unknown escape
/// (`a\`, `a\x`) and for an empty key (`a..b`, `.a`, `a.`).
///
/// # Arguments
/// * `name` - The parameter name
pub fn split_path(name: &str) -> Option<Vec<String>> {
    let mut keys = vec![String::new()];
    let mut chars = name.chars();
    while let Some(c) = chars.next() {
        match c {
            '.' => keys.push(String::new()),
            '\\' => match chars.next()? {
                escaped @ ('.' | '\\') => keys.last_mut()?.push(escaped),
                _ => return None,
            },
            other => keys.last_mut()?.push(other),
        }
    }
    keys.iter().all(|key| !key.is_empty()).then_some(keys)
}

/// The rendered value of a parameter as a JSON value for a Snakemake configuration file:
/// numbers as numbers, a true/false choice as a boolean, everything else as a string.
///
/// # Arguments
/// * `parameter` - The parameter, decides how the value is read
/// * `plain` - Its rendered value, see [`value_string`]
fn configuration_value(parameter: &Parameter, plain: String) -> Value {
    let number = || serde_json::from_str::<serde_json::Number>(&plain).ok();
    match parameter {
        Parameter::Integer(_) | Parameter::Float(_) => {
            number().map_or(Value::String(plain), Value::Number)
        }
        Parameter::ValueSelect(_) if plain == "true" => Value::Bool(true),
        Parameter::ValueSelect(_) if plain == "false" => Value::Bool(false),
        _ => Value::String(plain),
    }
}

/// Puts `value` into the nested object `root` at the place the keys say, creating the objects
/// on the way. Fails with [`Error::InvalidName`] (`name`) if a key on the way holds a value or
/// the last key is taken already.
///
/// # Arguments
/// * `root` - The object to add to
/// * `keys` - Path of keys, at least one
/// * `value` - The value for the last key
/// * `name` - The parameter name, for the error
fn insert_nested(
    root: &mut serde_json::Map<String, Value>,
    keys: &[String],
    value: Value,
    name: &str,
) -> Result<(), Error> {
    let invalid = || Error::InvalidName(name.to_string());
    let (last, parents) = keys.split_last().ok_or_else(invalid)?;
    let mut level = root;
    for key in parents {
        level = level
            .entry(key.clone())
            .or_insert_with(|| Value::Object(Default::default()))
            .as_object_mut()
            .ok_or_else(invalid)?;
    }
    if level.contains_key(last) {
        return Err(invalid());
    }
    level.insert(last.clone(), value);
    Ok(())
}

/// The value of a parameter as a JSON value of a CWL input object, `None` if unset. Like
/// [`configuration_value`], except that `Path` values are `File` or `Directory` objects
/// (`{"class": "File", "path": ...}`, an array of them for several paths): CWL inputs of these
/// types do not accept plain strings. A path that is a directory on this host becomes a
/// `Directory`, every other one a `File`.
///
/// # Arguments
/// * `parameter` - The parameter, decides how the value is read
/// * `reference_directory` - Absolute directory of the global reference data, see [`value_string`]
/// * `project_directory` - Absolute project directory the relative paths are joined onto (CWL
///   resolves relative paths against the input file, not the working directory); `None` leaves
///   them relative
/// * `mapping` - How host paths look inside the container; whether a path is a directory is
///   decided on the host path, the object holds the container-side form
fn cwl_value(
    parameter: &Parameter,
    reference_directory: Option<&Path>,
    project_directory: Option<&Path>,
    mapping: PathMapping,
) -> Result<Option<Value>, Error> {
    let Parameter::Path(p) = parameter else {
        return Ok(value_string(parameter, reference_directory, mapping)?
            .map(|plain| configuration_value(parameter, plain)));
    };
    let Some(paths) = p.value() else {
        return Ok(None);
    };
    let mut objects: Vec<Value> = resolve_paths(parameter, paths, reference_directory)?
        .into_iter()
        .map(|resolved| match project_directory {
            Some(base) => {
                let host = base.join(&resolved.host);
                (mapping.container(&host), host)
            }
            None => (resolved.text(mapping), resolved.host),
        })
        .map(|(text, host)| {
            let class = if host.is_dir() { "Directory" } else { "File" };
            serde_json::json!({ "class": class, "path": text })
        })
        .collect();
    Ok(match objects.len() {
        0 => None,
        1 => objects.pop(),
        _ => Some(Value::Array(objects)),
    })
}

/// The workflow parameters of a Nextflow, Snakemake or CWL workflow as the content of the file the
/// engine reads them from (`-params-file` for Nextflow, `--configfile` for Snakemake, the input
/// object for CWL; JSON is valid YAML). Passing them on the command line would make the engine parse every value again
/// (a text `7` becomes a number, `a: b` a dictionary, a line break a space). Names with dots
/// (`ref.species`) are nested objects, `\.` is a literal dot (see [`split_path`]). Numbers and
/// true/false choices keep their type, all other values are strings. Parameters without a value
/// are skipped.
///
/// `None` if no parameter has a value, and always for an engine that takes arguments instead
/// (Container, see [`render_args`]). Fails with [`Error::InvalidName`] for a malformed name (CWL:
/// also one with a dot, input names are flat) or one that is both a value and an object (`a` and `a.b`), and with the errors of
/// [`value_string`]. Call it next to [`render_args`] to check a configuration before a run.
///
/// # Arguments
/// * `config` - The workflow configuration with the parameter values applied
/// * `reference_directory` - Absolute directory of the global reference data, see [`value_string`]
/// * `project_directory` - Absolute project directory, CWL paths are made absolute with it (the
///   other engines run in it, so their relative paths work as they are); `None` leaves them
///   relative, enough to check a configuration
/// * `mapping` - How host paths look inside the container, see [`PathMapping`]
pub fn render_parameter_file(
    config: &WorkflowConfiguration,
    reference_directory: Option<&Path>,
    project_directory: Option<&Path>,
    mapping: PathMapping,
) -> Result<Option<Value>, Error> {
    if !matches!(
        config.engine(),
        Engine::Nextflow | Engine::Snakemake | Engine::Cwl
    ) {
        return Ok(None);
    }
    let is_cwl = config.engine() == &Engine::Cwl;
    let mut root = serde_json::Map::new();
    for p in config
        .fixed_parameters()
        .iter()
        .chain(config.dynamic_parameters())
        .filter(|p| !matches!(p, Parameter::Group(_)))
    {
        validate_name(p.name())?;
        let keys = split_path(p.name()).ok_or_else(|| Error::InvalidName(p.name().to_string()))?;
        if is_cwl {
            // The names of a CWL input object are flat
            if keys.len() != 1 {
                return Err(Error::InvalidName(p.name().to_string()));
            }
            if let Some(value) = cwl_value(p, reference_directory, project_directory, mapping)? {
                insert_nested(&mut root, &keys, value, p.name())?;
            }
        } else if let Some(plain) = value_string(p, reference_directory, mapping)? {
            insert_nested(&mut root, &keys, configuration_value(p, plain), p.name())?;
        }
    }
    Ok((!root.is_empty()).then_some(Value::Object(root)))
}

/// Flag for a Container parameter: single character names are short options (`-v`), longer
/// ones long options (`--verbose`).
fn container_flag(name: &str) -> String {
    if name.chars().count() == 1 {
        format!("-{name}")
    } else {
        format!("--{name}")
    }
}

/// Renders engine parameters and fixed + dynamic parameters into engine arguments (argv, not a
/// shell string: pass them to the process directly, no quoting needed).
///
/// * Nextflow: engine parameters `-name value`, workflow parameters are not arguments, see
///   [`render_parameter_file`]
/// * Snakemake: engine parameters `--name value`, workflow parameters are not arguments, see
///   [`render_parameter_file`]
/// * CWL (Toil): engine parameters `--name value`, workflow parameters are the input object of
///   [`render_parameter_file`]
///
/// * Container: no engine parameters, workflow parameters `-n value` (single character name) or
///   `--name value`, passed to the image
///
/// Parameters without a value are skipped. Executable, script and source are up to the executor.
///
/// # Arguments
/// * `config` - The workflow configuration with the parameter values applied
/// * `reference_directory` - Absolute directory of the global reference data, see [`value_string`]
/// * `mapping` - How host paths look inside the container, see [`PathMapping`]
pub fn render_args(
    config: &WorkflowConfiguration,
    reference_directory: Option<&Path>,
    mapping: PathMapping,
) -> Result<Vec<String>, Error> {
    let engine_prefix = match config.engine() {
        Engine::Nextflow => "-",
        Engine::Snakemake | Engine::Cwl => "--",
        Engine::Container => {
            if !config.engine_parameters().is_empty() {
                return Err(Error::EngineParametersUnsupported(
                    Engine::Container.to_string(),
                ));
            }
            return render_container_args(config, reference_directory, mapping);
        }
    };

    let mut args = Vec::new();
    for p in config.engine_parameters() {
        validate_name(p.name())?;
        args.push(format!("{engine_prefix}{}", p.name()));
        if !p.value().is_empty() {
            args.push(p.value().to_string());
        }
    }

    Ok(args)
}

/// The command line arguments of a `Container` workflow, see [`render_args`].
///
/// # Arguments
/// * `config` - The workflow configuration with the parameter values applied
/// * `reference_directory` - Absolute directory of the global reference data
/// * `mapping` - How host paths look inside the container
fn render_container_args(
    config: &WorkflowConfiguration,
    reference_directory: Option<&Path>,
    mapping: PathMapping,
) -> Result<Vec<String>, Error> {
    let mut args = Vec::new();
    for p in config
        .fixed_parameters()
        .iter()
        .chain(config.dynamic_parameters())
        .filter(|p| !matches!(p, Parameter::Group(_)))
    {
        validate_name(p.name())?;
        if let Some(v) = value_string(p, reference_directory, mapping)? {
            args.push(container_flag(p.name()));
            args.push(v);
        }
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::workflow_configuration::{
        engine::EngineParameter,
        parameters::{
            group::Group,
            number::Integer,
            path,
            text::Text,
            value_select::{SelectOption, ValueSelect},
        },
    };

    fn pv(name: &str, value: Value) -> ParameterValue {
        ParameterValue {
            name: name.into(),
            value,
        }
    }

    fn config(engine: Engine) -> WorkflowConfiguration {
        let mut c = WorkflowConfiguration::default();
        *c.engine_mut() = engine;
        let mut int = Integer::new("threads".into(), "T".into(), "".into());
        *int.min_mut() = Some(1);
        *int.max_mut() = Some(8);
        let mut fixed = Text::new("mode".into(), "M".into(), "".into());
        *fixed.value_mut() = Some("fast".into());
        let mut p = path::Path::new("input".into(), "I".into(), "".into());
        *p.limit_mut() = std::num::NonZeroUsize::new(2).unwrap();
        c.dynamic_parameters_mut().extend([
            Parameter::Integer(int),
            Parameter::Text(Text::new("name".into(), "N".into(), "".into())),
            Parameter::Path(p),
        ]);
        c.fixed_parameters_mut().push(Parameter::Text(fixed));
        c
    }

    #[test]
    fn nextflow() {
        let mut c = config(Engine::Nextflow);
        c.engine_parameters_mut()
            .push(EngineParameter::new("profile".into(), "docker".into()));
        c.engine_parameters_mut()
            .push(EngineParameter::new("resume".into(), "".into()));
        apply_values(
            &mut c,
            &[
                pv("threads", json!(4)),
                pv("input", json!(["a.fq", "b.fq"])),
            ],
        )
        .unwrap();
        assert_eq!(
            render_args(&c, None, PathMapping::Native).unwrap(),
            ["-profile", "docker", "-resume"]
        );
        assert_eq!(
            render_parameter_file(&c, None, None, PathMapping::Native).unwrap(),
            Some(json!({
                "mode": "fast", "threads": 4, "input": "a.fq,b.fq"
            }))
        );
    }

    #[test]
    fn snakemake() {
        let mut c = config(Engine::Snakemake);
        apply_values(&mut c, &[pv("threads", json!(2)), pv("name", json!("x y"))]).unwrap();
        assert!(
            render_args(&c, None, PathMapping::Native)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            render_parameter_file(&c, None, None, PathMapping::Native).unwrap(),
            Some(json!({"mode": "fast", "threads": 2, "name": "x y"}))
        );
    }

    /// Values Snakemake would parse on the command line stay exactly as typed.
    #[test]
    fn snakemake_values_are_not_reinterpreted() {
        let mut c = config(Engine::Snakemake);
        c.fixed_parameters_mut().clear();
        c.dynamic_parameters_mut().clear();
        for (name, value) in [
            ("a", "line one\nline two"),
            ("b", "a: b"),
            ("c", "7"),
            ("d", "say \"hi\""),
            ("e", "[1,2]"),
        ] {
            let mut text = Text::new(name.into(), "".into(), "".into());
            *text.value_mut() = Some(value.into());
            c.dynamic_parameters_mut().push(Parameter::Text(text));
        }
        assert_eq!(
            render_parameter_file(&c, None, None, PathMapping::Native).unwrap(),
            Some(json!({
                "a": "line one\nline two", "b": "a: b", "c": "7",
                "d": "say \"hi\"", "e": "[1,2]"
            }))
        );
    }

    /// Dotted names are nested into one object per top level name.
    #[test]
    fn snakemake_nested_names_render_as_objects() {
        let mut c = config(Engine::Snakemake);
        c.fixed_parameters_mut().clear();
        let mut select = ValueSelect::new("trimming.activate".into(), "".into(), "".into());
        *select.options_mut() = ["true", "false"]
            .map(|v| SelectOption {
                label: v.into(),
                value: v.into(),
            })
            .to_vec();
        c.dynamic_parameters_mut().extend([
            Parameter::Text(Text::new("ref.species".into(), "".into(), "".into())),
            Parameter::Integer(Integer::new("ref.release".into(), "".into(), "".into())),
            Parameter::ValueSelect(select),
            Parameter::Text(Text::new("params.star.index".into(), "".into(), "".into())),
        ]);
        apply_values(
            &mut c,
            &[
                pv("threads", json!(2)),
                pv("ref.species", json!("it's")),
                pv("ref.release", json!(100)),
                pv("trimming.activate", json!(["false"])),
                pv("params.star.index", json!("a\\b")),
            ],
        )
        .unwrap();
        assert_eq!(
            render_parameter_file(&c, None, None, PathMapping::Native).unwrap(),
            Some(json!({
                "threads": 2,
                "ref": {"species": "it's", "release": 100},
                "trimming": {"activate": false},
                "params": {"star": {"index": "a\\b"}},
            }))
        );
    }

    /// A name that is a value and a dictionary, or has an empty segment, is rejected.
    #[test]
    fn snakemake_conflicting_names_are_rejected() {
        for names in [["a", "a.b"], ["a.b", "a"], ["a..b", "x"], ["a.", "x"]] {
            let mut c = config(Engine::Snakemake);
            c.fixed_parameters_mut().clear();
            c.dynamic_parameters_mut().clear();
            for name in names {
                let mut text = Text::new(name.into(), "".into(), "".into());
                *text.value_mut() = Some("v".into());
                c.dynamic_parameters_mut().push(Parameter::Text(text));
            }
            assert!(
                matches!(
                    render_parameter_file(&c, None, None, PathMapping::Native),
                    Err(Error::InvalidName(_))
                ),
                "{names:?}"
            );
        }
    }

    /// Escaped dots and backslashes stay inside one key, unescaped dots separate keys.
    #[test]
    fn paths_split_on_unescaped_dots() {
        assert_eq!(split_path("a.b"), Some(vec!["a".into(), "b".into()]));
        assert_eq!(split_path(r"a\.b.c"), Some(vec!["a.b".into(), "c".into()]));
        assert_eq!(split_path(r"a\\.b"), Some(vec![r"a\".into(), "b".into()]));
        for bad in ["", ".a", "a.", "a..b", r"a\", r"a\x"] {
            assert_eq!(split_path(bad), None, "{bad:?}");
        }
        for key in ["plain", "a.b", r"x\y", r"\.", "..", "a.\\.b"] {
            assert_eq!(
                split_path(&escape_path_segment(key)),
                Some(vec![key.to_string()])
            );
        }
    }

    /// A key with a dot renders as one key of the dictionary, not as a nested one.
    #[test]
    fn snakemake_dotted_key_stays_one_key() {
        let mut c = config(Engine::Snakemake);
        c.fixed_parameters_mut().clear();
        c.dynamic_parameters_mut().clear();
        for name in [r"ref.v1\.2", r"ref.v1\.3"] {
            let mut text = Text::new(name.into(), "".into(), "".into());
            *text.value_mut() = Some("x".into());
            c.dynamic_parameters_mut().push(Parameter::Text(text));
        }
        assert_eq!(
            render_parameter_file(&c, None, None, PathMapping::Native).unwrap(),
            Some(json!({"ref": {"v1.2": "x", "v1.3": "x"}}))
        );
    }

    #[test]
    fn unset_is_skipped() {
        let mut c = config(Engine::Snakemake);
        c.fixed_parameters_mut().clear();
        assert!(
            render_args(&c, None, PathMapping::Native)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            render_parameter_file(&c, None, None, PathMapping::Native).unwrap(),
            None
        );
    }

    #[test]
    fn rejects_bad_input() {
        let mut c = config(Engine::Nextflow);
        assert_eq!(
            apply_values(&mut c, &[pv("nope", json!(1))]),
            Err(Error::UnknownParameter("nope".into()))
        );
        assert!(matches!(
            apply_values(&mut c, &[pv("threads", json!("4"))]),
            Err(Error::TypeMismatch(..))
        ));
        assert!(matches!(
            apply_values(&mut c, &[pv("threads", json!(9))]),
            Err(Error::OutOfRange(..))
        ));
        assert!(matches!(
            apply_values(&mut c, &[pv("input", json!(["a", "b", "c"]))]),
            Err(Error::OutOfRange(..))
        ));
    }

    #[test]
    fn rejects_flag_injection_in_names() {
        let mut c = config(Engine::Nextflow);
        *c.dynamic_parameters_mut()[1].name_mut() = "-x".into();
        apply_values(&mut c, &[pv("-x", json!("v"))]).unwrap();
        assert_eq!(
            render_parameter_file(&c, None, None, PathMapping::Native),
            Err(Error::InvalidName("-x".into()))
        );
    }

    #[test]
    fn container_flags_by_name_length() {
        let mut c = config(Engine::Container);
        let mut short = Text::new("n".into(), "N".into(), "".into());
        *short.value_mut() = Some("3".into());
        c.fixed_parameters_mut().push(Parameter::Text(short));
        assert_eq!(
            render_args(&c, None, PathMapping::Native).unwrap(),
            ["--mode", "fast", "-n", "3"]
        );
    }

    #[test]
    fn container_rejects_engine_parameters() {
        let mut c = config(Engine::Container);
        c.engine_parameters_mut()
            .push(EngineParameter::new("memory".into(), "2g".into()));
        assert!(matches!(
            render_args(&c, None, PathMapping::Native),
            Err(Error::EngineParametersUnsupported(_))
        ));
    }

    // Native mapping with POSIX host paths: a Windows host uses a mapped style (see the `PathMapping` tests).
    #[cfg(unix)]
    #[test]
    fn cwl() {
        let mut c = config(Engine::Cwl);
        c.engine_parameters_mut()
            .push(EngineParameter::new("maxCores".into(), "2".into()));
        apply_values(
            &mut c,
            &[
                pv("threads", json!(2)),
                pv("name", json!("x y")),
                pv("input", json!(["a.fq", "b.fq"])),
            ],
        )
        .unwrap();
        assert_eq!(
            render_args(&c, None, PathMapping::Native).unwrap(),
            ["--maxCores", "2"]
        );
        assert_eq!(
            render_parameter_file(&c, None, Some(Path::new("/no/such/p")), PathMapping::Native)
                .unwrap(),
            Some(json!({
                "mode": "fast", "threads": 2, "name": "x y",
                "input": [
                    {"class": "File", "path": "/no/such/p/a.fq"},
                    {"class": "File", "path": "/no/such/p/b.fq"}
                ]
            }))
        );
    }

    /// An existing directory is a `Directory`, a single path is not wrapped in an array.
    #[test]
    fn cwl_path_classes() {
        let project = std::env::temp_dir().canonicalize().unwrap();
        let mut c = config(Engine::Cwl);
        // A project-relative path of an existing folder: the project itself is the folder
        let name = project.file_name().unwrap().to_str().unwrap().to_string();
        let parent = project.parent().unwrap();
        apply_values(&mut c, &[pv("input", json!([name.clone()]))]).unwrap();
        let rendered = render_parameter_file(&c, None, Some(parent), PathMapping::Native)
            .unwrap()
            .unwrap();
        assert_eq!(
            rendered["input"],
            json!({"class": "Directory", "path": project.to_str().unwrap()})
        );
    }

    /// CWL input names are flat: no nesting through dots.
    #[test]
    fn cwl_rejects_nested_names() {
        let mut c = config(Engine::Cwl);
        c.fixed_parameters_mut().push(Parameter::Text({
            let mut t = Text::new("a.b".into(), "A".into(), "".into());
            *t.value_mut() = Some("1".into());
            t
        }));
        assert!(matches!(
            render_parameter_file(&c, None, None, PathMapping::Native),
            Err(Error::InvalidName(_))
        ));
    }

    fn select(multi: bool) -> ValueSelect {
        let mut s = ValueSelect::new("mode2".into(), "M".into(), "".into());
        s.options_mut().extend(["a", "b"].map(|v| SelectOption {
            label: v.to_uppercase(),
            value: v.into(),
        }));
        *s.is_multiselect_mut() = multi;
        s
    }

    #[test]
    fn value_select() {
        let mut c = config(Engine::Nextflow);
        c.dynamic_parameters_mut()
            .push(Parameter::ValueSelect(select(false)));
        apply_values(&mut c, &[pv("mode2", json!("b"))]).unwrap();
        assert_eq!(
            render_parameter_file(&c, None, None, PathMapping::Native)
                .unwrap()
                .unwrap()["mode2"],
            json!("b")
        );
        assert!(matches!(
            apply_values(&mut c, &[pv("mode2", json!("z"))]),
            Err(Error::OutOfRange(..))
        ));
        assert!(matches!(
            apply_values(&mut c, &[pv("mode2", json!(["a", "b"]))]),
            Err(Error::OutOfRange(..))
        ));
    }

    #[test]
    fn value_select_multi() {
        let mut c = config(Engine::Nextflow);
        c.dynamic_parameters_mut()
            .push(Parameter::ValueSelect(select(true)));
        apply_values(&mut c, &[pv("mode2", json!(["a", "b"]))]).unwrap();
        assert_eq!(
            render_parameter_file(&c, None, None, PathMapping::Native)
                .unwrap()
                .unwrap()["mode2"],
            json!("a,b")
        );
        assert_eq!(
            value_json(c.dynamic_parameters().last().unwrap()),
            json!(["a", "b"])
        );
    }

    #[test]
    fn group_is_skipped() {
        let mut c = config(Engine::Nextflow);
        c.dynamic_parameters_mut()
            .push(Parameter::Group(Group::new("S".into(), "".into())));
        assert_eq!(
            render_parameter_file(&c, None, None, PathMapping::Native).unwrap(),
            Some(json!({"mode": "fast"}))
        );
    }

    #[test]
    fn path_is_confined() {
        let mut c = config(Engine::Nextflow);
        assert!(matches!(
            apply_values(&mut c, &[pv("input", json!(["../x"]))]),
            Err(Error::InvalidPath(_))
        ));
        assert!(matches!(
            apply_values(&mut c, &[pv("input", json!(""))]),
            Err(Error::InvalidPath(_))
        ));
        apply_values(&mut c, &[pv("input", json!("/a/b.fq"))]).unwrap();
        assert_eq!(
            render_parameter_file(&c, None, None, PathMapping::Native)
                .unwrap()
                .unwrap()["input"],
            json!("a/b.fq")
        );
    }

    /// A path not matching the parameter's pattern is rejected; the lenient variant skips it.
    #[test]
    fn path_pattern_is_enforced() {
        let mut c = config(Engine::Nextflow);
        let Some(Parameter::Path(p)) = c
            .dynamic_parameters_mut()
            .iter_mut()
            .find(|p| p.name() == "input")
        else {
            panic!("no input parameter");
        };
        *p.is_excepting_files_mut() = true;
        p.set_file_pattern(r".*\.(fastq|fq)(\.gz)?");
        apply_values(&mut c, &[pv("input", json!(["a/x.fq.gz", "y.fastq"]))]).unwrap();
        assert!(matches!(
            apply_values(&mut c, &[pv("input", json!(["a/x.txt"]))]),
            Err(Error::PatternMismatch(..))
        ));
        assert_eq!(
            apply_values_lenient(&mut c, &[pv("input", json!(["a/x.txt"]))]),
            ["input"]
        );
    }

    /// On a Windows host the reference directory and the project directory reach the engine in
    /// the container-side form of the runtime, relative values stay as they are.
    #[test]
    fn windows_host_paths_are_mapped_for_the_container() {
        let mut c = config(Engine::Nextflow);
        apply_values(
            &mut c,
            &[pv(
                "input",
                json!(["macworp_global_reference_data/hg38/a.fa", "reads/b.fq"]),
            )],
        )
        .unwrap();
        let reference = Path::new(r"C:\Data\Reference");
        for (mapping, expected) in [
            (
                PathMapping::WslMount,
                "/mnt/c/Data/Reference/hg38/a.fa,reads/b.fq",
            ),
            (
                PathMapping::DriveRoot,
                "/c/Data/Reference/hg38/a.fa,reads/b.fq",
            ),
        ] {
            assert_eq!(
                render_parameter_file(&c, Some(reference), None, mapping)
                    .unwrap()
                    .unwrap()["input"],
                json!(expected)
            );
        }
    }

    /// CWL input objects hold container-side paths built from the project directory; a path
    /// that does not exist on the host is a `File`.
    #[test]
    fn windows_host_project_paths_are_mapped_for_cwl() {
        let mut c = config(Engine::Cwl);
        apply_values(&mut c, &[pv("input", json!(["a.fq"]))]).unwrap();
        let project = Path::new(r"C:\Users\Admin\p1");
        let rendered = render_parameter_file(&c, None, Some(project), PathMapping::DriveRoot)
            .unwrap()
            .unwrap();
        assert_eq!(
            rendered["input"],
            json!({"class": "File", "path": "/c/Users/Admin/p1/a.fq"})
        );
    }

    /// Values below the reference prefix become absolute reference paths, other values are
    /// untouched, `..` still cannot escape, and a missing reference directory is an error.
    // Native mapping with POSIX host paths: a Windows host uses a mapped style (see the `PathMapping` tests).
    #[cfg(unix)]
    #[test]
    fn reference_prefix_is_rewritten_at_render() {
        let mut c = config(Engine::Nextflow);
        apply_values(
            &mut c,
            &[pv(
                "input",
                json!(["macworp_global_reference_data/hg38/a.fa", "reads/b.fq"]),
            )],
        )
        .unwrap();
        let reference = Path::new("/data/reference");
        assert_eq!(
            render_parameter_file(&c, Some(reference), None, PathMapping::Native)
                .unwrap()
                .unwrap()["input"],
            json!("/data/reference/hg38/a.fa,reads/b.fq")
        );
        assert_eq!(
            render_parameter_file(&c, None, None, PathMapping::Native),
            Err(Error::ReferenceUnavailable("input".into()))
        );
        assert!(matches!(
            apply_values(
                &mut c,
                &[pv("input", json!("macworp_global_reference_data/../../x"))]
            ),
            Err(Error::InvalidPath(_))
        ));
    }

    #[test]
    fn lenient_skips_stale_values() {
        let mut c = config(Engine::Nextflow);
        let skipped = apply_values_lenient(
            &mut c,
            &[
                pv("threads", json!(4)),
                pv("gone", json!("x")),
                pv("name", json!(5)),
                pv("input", json!(["a.fq"])),
            ],
        );
        assert_eq!(skipped, ["gone", "name"]);
        assert_eq!(value_json(&c.dynamic_parameters()[0]), json!(4));
        assert_eq!(value_json(&c.dynamic_parameters()[2]), json!(["a.fq"]));
    }
}
