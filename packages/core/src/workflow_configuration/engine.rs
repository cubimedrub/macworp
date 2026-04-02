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
    Docker,
}

impl Display for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Engine::Nextflow => write!(f, "Nextflow"),
            Engine::Snakemake => write!(f, "Snakemake"),
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
            "docker" => Ok(Engine::Docker),
            _ => Err(Error::UnsupportedEngine(s.to_string())),
        }
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
