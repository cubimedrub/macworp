use std::{
    fmt::Display,
    ops::{Deref, DerefMut},
    str::FromStr,
};

use serde::{Deserialize, Serialize};
use strum::EnumIter;
use thiserror::Error;

use crate::workflow_configuration::parameters::parameter::IsParameter;

/// Glob paramter for collecting multiple files
pub mod glob;
/// Float and integer paramter
pub mod number;
/// Generic paramerter for simple data
pub mod parameter;
pub mod path;
/// Visual divider without value
pub mod separator;
/// Single and multiline text
pub mod text;
/// Choice of fixed options
pub mod value_select;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Unsupported parameter type `{0}`")]
    UnsupporterParameter(String),
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, EnumIter)]
pub enum Parameter {
    /// A single line of text
    Text(text::Text),
    /// A integer
    Integer(number::Integer),
    /// A float
    Float(number::Float),
    /// A glob pattern for collecting multiple files
    Glob(glob::Glob),
    /// A path to a file or folder
    Path(path::Path),
    /// Divider in the start dialog, carries no value
    Separator(separator::Separator),
    /// One or several of a fixed set of options
    ValueSelect(value_select::ValueSelect),
}

impl Parameter {
    pub fn is_same_variant(&self, other: &Self) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

impl Deref for Parameter {
    type Target = dyn IsParameter;

    fn deref(&self) -> &Self::Target {
        match self {
            Parameter::Text(text) => text,
            Parameter::Integer(integer) => integer,
            Parameter::Float(float) => float,
            Parameter::Glob(glob) => glob,
            Parameter::Path(path) => path,
            Parameter::Separator(separator) => separator,
            Parameter::ValueSelect(select) => select,
        }
    }
}

impl DerefMut for Parameter {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Parameter::Text(text) => text,
            Parameter::Integer(integer) => integer,
            Parameter::Float(float) => float,
            Parameter::Glob(glob) => glob,
            Parameter::Path(path) => path,
            Parameter::Separator(separator) => separator,
            Parameter::ValueSelect(select) => select,
        }
    }
}

impl Display for Parameter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Parameter::Text(_) => "Text",
            Parameter::Integer(_) => "Integer",
            Parameter::Float(_) => "Float",
            Parameter::Glob(_) => "Glob",
            Parameter::Path(_) => "Path",
            Parameter::Separator(_) => "Separator",
            Parameter::ValueSelect(_) => "ValueSelect",
        };

        write!(f, "{text}")
    }
}

impl FromStr for Parameter {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "text" => Ok(Parameter::Text(text::Text::default())),
            "integer" => Ok(Parameter::Integer(number::Integer::default())),
            "float" => Ok(Parameter::Float(number::Float::default())),
            "glob" => Ok(Parameter::Glob(glob::Glob::default())),
            "path" => Ok(Parameter::Path(path::Path::default())),
            "separator" => Ok(Parameter::Separator(separator::Separator::default())),
            "valueselect" | "value-select" => {
                Ok(Parameter::ValueSelect(value_select::ValueSelect::default()))
            }
            _ => Err(Error::UnsupporterParameter(s.to_string())),
        }
    }
}

impl Default for Parameter {
    fn default() -> Self {
        Self::Text(text::Text::default())
    }
}
