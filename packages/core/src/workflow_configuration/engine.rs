use std::{fmt::Display, str::FromStr};

use serde::{Deserialize, Serialize};
use strum::EnumIter;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Workflow engine `{0}` is not supported")]
    UnsupportedEngine(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, EnumIter)]
pub enum Engine {
    Nextflow,
    Snakemake,
    Cwl,
    Docker,
}

impl Display for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Engine::Nextflow => write!(f, "Nextflow"),
            Engine::Snakemake => write!(f, "Snakemake"),
            Engine::Cwl => write!(f, "Cwl"),
            Engine::Docker => write!(f, "Docker"),
        }
    }
}

impl FromStr for Engine {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "nextflow" => Ok(Engine::Nextflow),
            "snakemake" => Ok(Engine::Snakemake),
            "cwl" => Ok(Engine::Cwl),
            "docker" => Ok(Engine::Docker),
            _ => Err(Error::UnsupportedEngine(s.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use strum::IntoEnumIterator;

    use super::*;

    #[test]
    fn display_from_str_roundtrip() {
        for engine in Engine::iter() {
            assert_eq!(engine.to_string().parse::<Engine>().unwrap(), engine);
        }
        assert_eq!("CWL".parse::<Engine>().unwrap(), Engine::Cwl);
    }

    #[test]
    fn cwl_serde_roundtrip() {
        let json = serde_json::to_string(&Engine::Cwl).unwrap();
        assert_eq!(json, "\"Cwl\"");
        assert_eq!(serde_json::from_str::<Engine>(&json).unwrap(), Engine::Cwl);
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EngineParameter {
    name: String,
    value: String,
}

impl EngineParameter {
    pub fn new(name: String, value: String) -> Self {
        Self { name, value }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn value(&self) -> &str {
        &self.value
    }
}
