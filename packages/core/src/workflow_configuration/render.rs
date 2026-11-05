use std::path::PathBuf;

use serde_json::Value;
use thiserror::Error;

use super::{
    WorkflowConfiguration,
    engine::Engine,
    parameters::{Parameter, parameter::IsParameterWithValue},
};
use crate::{
    paths::{is_confined, sanitize_relative},
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
    /// A value for a parameter that takes none (separator)
    #[error("Parameter `{0}` takes no value")]
    NoValueAllowed(String),
    /// Path points out of the project directory (`..`) or is empty (parameter name)
    #[error("Path in parameter `{0}` must stay inside the project directory")]
    InvalidPath(String),
    /// Number outside `min`/`max` or more paths than `limit` (parameter name, reason)
    #[error("Value of parameter `{0}` is out of range: {1}")]
    OutOfRange(String, String),
    /// Name is empty, starts with `-` or has characters other than letters, digits, `_`, `.`, `-`
    #[error("Invalid parameter name `{0}`, use letters, digits, `_`, `.` and `-` (not leading)")]
    InvalidName(String),
    /// The engine takes no engine parameters (Container)
    #[error("Engine `{0}` does not support engine parameters")]
    EngineParametersUnsupported(String),
    /// Rendering exists only for Nextflow, Snakemake and Container so far
    #[error("Command line rendering is not supported for engine `{0}` yet")]
    UnsupportedEngine(String),
}

/// Names end up in command line flags, so they must not be able to form another flag or contain
/// separators.
fn validate_name(name: &str) -> Result<(), Error> {
    let valid = !name.is_empty()
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
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
        Parameter::Separator(_) => {
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
        Parameter::Separator(_) => Value::Null,
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
/// run): values for unknown names, separators, or of a type or range that no longer fits are
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

/// Renders the value of a parameter as it is passed on the command line, `None` if unset.
fn value_string(parameter: &Parameter) -> Option<String> {
    match parameter {
        Parameter::Text(p) => p.value().cloned(),
        Parameter::Glob(p) => p.value().cloned(),
        Parameter::Integer(p) => p.value().map(|v| v.to_string()),
        Parameter::Float(p) => p.value().map(|v| v.to_string()),
        Parameter::Path(p) => p.value().map(|paths| {
            paths
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(",")
        }),
        Parameter::ValueSelect(p) => p.value().map(|values| values.join(",")),
        Parameter::Separator(_) => None,
    }
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
/// * Nextflow: engine parameters `-name value`, workflow parameters `--name value`
/// * Snakemake: engine parameters `--name value`, workflow parameters `--config name=value ...`
///
/// * Container: no engine parameters, workflow parameters `-n value` (single character name) or
///   `--name value`, passed to the image
///
/// Parameters without a value are skipped. Executable, script and source are up to the executor.
pub fn render_args(config: &WorkflowConfiguration) -> Result<Vec<String>, Error> {
    let engine_prefix = match config.engine() {
        Engine::Nextflow => "-",
        Engine::Snakemake => "--",
        Engine::Container => {
            if !config.engine_parameters().is_empty() {
                return Err(Error::EngineParametersUnsupported(
                    Engine::Container.to_string(),
                ));
            }
            return render_container_args(config);
        }
        other => return Err(Error::UnsupportedEngine(other.to_string())),
    };

    let mut args = Vec::new();
    for p in config.engine_parameters() {
        validate_name(p.name())?;
        args.push(format!("{engine_prefix}{}", p.name()));
        if !p.value().is_empty() {
            args.push(p.value().to_string());
        }
    }

    let params = config
        .fixed_parameters()
        .iter()
        .chain(config.dynamic_parameters())
        .filter(|p| !matches!(p, Parameter::Separator(_)));
    match config.engine() {
        Engine::Nextflow => {
            for p in params {
                validate_name(p.name())?;
                if let Some(v) = value_string(p) {
                    args.push(format!("--{}", p.name()));
                    args.push(v);
                }
            }
        }
        _ => {
            let mut pairs = Vec::new();
            for p in params {
                validate_name(p.name())?;
                if let Some(v) = value_string(p) {
                    pairs.push(format!("{}={v}", p.name()));
                }
            }
            if !pairs.is_empty() {
                args.push("--config".to_string());
                args.extend(pairs);
            }
        }
    }
    Ok(args)
}

fn render_container_args(config: &WorkflowConfiguration) -> Result<Vec<String>, Error> {
    let mut args = Vec::new();
    for p in config
        .fixed_parameters()
        .iter()
        .chain(config.dynamic_parameters())
        .filter(|p| !matches!(p, Parameter::Separator(_)))
    {
        validate_name(p.name())?;
        if let Some(v) = value_string(p) {
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
            number::Integer,
            path,
            separator::Separator,
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
            render_args(&c).unwrap(),
            [
                "-profile",
                "docker",
                "-resume",
                "--mode",
                "fast",
                "--threads",
                "4",
                "--input",
                "a.fq,b.fq"
            ]
        );
    }

    #[test]
    fn snakemake() {
        let mut c = config(Engine::Snakemake);
        apply_values(&mut c, &[pv("threads", json!(2)), pv("name", json!("x y"))]).unwrap();
        assert_eq!(
            render_args(&c).unwrap(),
            ["--config", "mode=fast", "threads=2", "name=x y"]
        );
    }

    #[test]
    fn unset_is_skipped() {
        let mut c = config(Engine::Snakemake);
        c.fixed_parameters_mut().clear();
        assert!(render_args(&c).unwrap().is_empty());
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
        assert_eq!(render_args(&c), Err(Error::InvalidName("-x".into())));
    }

    #[test]
    fn container_flags_by_name_length() {
        let mut c = config(Engine::Container);
        let mut short = Text::new("n".into(), "N".into(), "".into());
        *short.value_mut() = Some("3".into());
        c.fixed_parameters_mut().push(Parameter::Text(short));
        assert_eq!(render_args(&c).unwrap(), ["--mode", "fast", "-n", "3"]);
    }

    #[test]
    fn container_rejects_engine_parameters() {
        let mut c = config(Engine::Container);
        c.engine_parameters_mut()
            .push(EngineParameter::new("memory".into(), "2g".into()));
        assert!(matches!(
            render_args(&c),
            Err(Error::EngineParametersUnsupported(_))
        ));
    }

    #[test]
    fn unsupported_engine() {
        assert!(matches!(
            render_args(&config(Engine::Cwl)),
            Err(Error::UnsupportedEngine(_))
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
        assert_eq!(&render_args(&c).unwrap()[2..], ["--mode2", "b"]);
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
        assert_eq!(&render_args(&c).unwrap()[2..], ["--mode2", "a,b"]);
        assert_eq!(
            value_json(c.dynamic_parameters().last().unwrap()),
            json!(["a", "b"])
        );
    }

    #[test]
    fn separator_is_skipped() {
        let mut c = config(Engine::Nextflow);
        c.dynamic_parameters_mut()
            .push(Parameter::Separator(Separator::new("S".into(), "".into())));
        assert_eq!(render_args(&c).unwrap(), ["--mode", "fast"]);
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
        assert_eq!(&render_args(&c).unwrap()[2..], ["--input", "a/b.fq"]);
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
