use std::path::PathBuf;

use serde_json::Value;
use thiserror::Error;

use super::{
    WorkflowConfiguration,
    engine::Engine,
    parameters::{Parameter, parameter::IsParameterWithValue},
};
use crate::requests::ParameterValue;

/// Why parameter values could not be applied or a configuration could not be rendered
#[derive(Debug, Error, PartialEq)]
pub enum RenderError {
    /// No dynamic parameter with this name
    #[error("Unknown parameter `{0}`")]
    UnknownParameter(String),
    /// The JSON value does not fit the parameter type (parameter name, expected type)
    #[error("Value of parameter `{0}` has the wrong type, expected {1}")]
    TypeMismatch(String, &'static str),
    /// Number outside `min`/`max` or more paths than `limit` (parameter name, reason)
    #[error("Value of parameter `{0}` is out of range: {1}")]
    OutOfRange(String, String),
    /// Name is empty, starts with `-` or has characters other than letters, digits, `_`, `.`, `-`
    #[error("Invalid parameter name `{0}`, use letters, digits, `_`, `.` and `-` (not leading)")]
    InvalidName(String),
    /// Rendering exists only for Nextflow and Snakemake so far
    #[error("Command line rendering is not supported for engine `{0}` yet")]
    UnsupportedEngine(String),
}

/// Names end up in command line flags, so they must not be able to form another flag or contain
/// separators.
fn validate_name(name: &str) -> Result<(), RenderError> {
    let valid = !name.is_empty()
        && !name.starts_with('-')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'));
    if valid {
        Ok(())
    } else {
        Err(RenderError::InvalidName(name.to_string()))
    }
}

/// Checks `min <= value <= max`, unset bounds are ignored
fn check_range<T: PartialOrd + std::fmt::Display + Copy>(
    name: &str,
    value: T,
    min: Option<T>,
    max: Option<T>,
) -> Result<(), RenderError> {
    if let Some(min) = min
        && value < min
    {
        return Err(RenderError::OutOfRange(
            name.to_string(),
            format!("{value} < minimum {min}"),
        ));
    }
    if let Some(max) = max
        && value > max
    {
        return Err(RenderError::OutOfRange(
            name.to_string(),
            format!("{value} > maximum {max}"),
        ));
    }
    Ok(())
}

/// Sets the value of one parameter from its JSON representation, `null` unsets it.
fn set_value(parameter: &mut Parameter, value: &Value) -> Result<(), RenderError> {
    let name = parameter.name().to_string();
    match parameter {
        Parameter::Text(p) => {
            *p.value_mut() = match value {
                Value::Null => None,
                Value::String(s) => Some(s.clone()),
                _ => return Err(RenderError::TypeMismatch(name, "string")),
            };
        }
        Parameter::Glob(p) => {
            *p.value_mut() = match value {
                Value::Null => None,
                Value::String(s) => Some(s.clone()),
                _ => return Err(RenderError::TypeMismatch(name, "string")),
            };
        }
        Parameter::Integer(p) => {
            *p.value_mut() = match value {
                Value::Null => None,
                v => {
                    let v = v
                        .as_i64()
                        .ok_or_else(|| RenderError::TypeMismatch(name.clone(), "integer"))?;
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
                        .ok_or_else(|| RenderError::TypeMismatch(name.clone(), "number"))?;
                    check_range(&name, v, p.min(), p.max())?;
                    Some(v)
                }
            };
        }
        Parameter::Path(p) => {
            let paths: Option<Vec<PathBuf>> = match value {
                Value::Null => None,
                Value::String(s) => Some(vec![PathBuf::from(s)]),
                Value::Array(items) => Some(
                    items
                        .iter()
                        .map(|i| {
                            i.as_str().map(PathBuf::from).ok_or_else(|| {
                                RenderError::TypeMismatch(name.clone(), "array of strings")
                            })
                        })
                        .collect::<Result<_, _>>()?,
                ),
                _ => return Err(RenderError::TypeMismatch(name, "array of strings")),
            };
            if let Some(paths) = &paths
                && paths.len() > p.limit().get()
            {
                return Err(RenderError::OutOfRange(
                    name,
                    format!("{} paths, at most {} allowed", paths.len(), p.limit()),
                ));
            }
            *p.value_mut() = paths;
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
    }
}

/// Sets user supplied values on the dynamic parameters of `config`, matched by name.
///
/// Fails on unknown names and on values of the wrong type or outside the allowed range.
/// Nothing but dynamic parameter values is touched.
pub fn apply_values(
    config: &mut WorkflowConfiguration,
    values: &[ParameterValue],
) -> Result<(), RenderError> {
    for v in values {
        let parameter = config
            .dynamic_parameters_mut()
            .iter_mut()
            .find(|p| p.name() == v.name)
            .ok_or_else(|| RenderError::UnknownParameter(v.name.clone()))?;
        set_value(parameter, &v.value)?;
    }
    Ok(())
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
    }
}

/// Renders engine parameters and fixed + dynamic parameters into engine arguments (argv, not a
/// shell string: pass them to the process directly, no quoting needed).
///
/// * Nextflow: engine parameters `-name value`, workflow parameters `--name value`
/// * Snakemake: engine parameters `--name value`, workflow parameters `--config name=value ...`
///
/// Parameters without a value are skipped. Executable, script and source are up to the executor.
pub fn render_args(config: &WorkflowConfiguration) -> Result<Vec<String>, RenderError> {
    let engine_prefix = match config.engine() {
        Engine::Nextflow => "-",
        Engine::Snakemake => "--",
        other => return Err(RenderError::UnsupportedEngine(other.to_string())),
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
        .chain(config.dynamic_parameters());
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::workflow_configuration::{
        engine::EngineParameter,
        parameters::{number::Integer, path, text::Text},
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
            Err(RenderError::UnknownParameter("nope".into()))
        );
        assert!(matches!(
            apply_values(&mut c, &[pv("threads", json!("4"))]),
            Err(RenderError::TypeMismatch(..))
        ));
        assert!(matches!(
            apply_values(&mut c, &[pv("threads", json!(9))]),
            Err(RenderError::OutOfRange(..))
        ));
        assert!(matches!(
            apply_values(&mut c, &[pv("input", json!(["a", "b", "c"]))]),
            Err(RenderError::OutOfRange(..))
        ));
    }

    #[test]
    fn rejects_flag_injection_in_names() {
        let mut c = config(Engine::Nextflow);
        *c.dynamic_parameters_mut()[1].name_mut() = "-x".into();
        apply_values(&mut c, &[pv("-x", json!("v"))]).unwrap();
        assert_eq!(render_args(&c), Err(RenderError::InvalidName("-x".into())));
    }

    #[test]
    fn unsupported_engine() {
        assert!(matches!(
            render_args(&config(Engine::Cwl)),
            Err(RenderError::UnsupportedEngine(_))
        ));
    }
}
