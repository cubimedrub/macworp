use serde::{Deserialize, Serialize};

/// Enum for workflow engines, e.g. Nextflow, Snakemake, CWL, Container
pub mod engine;
/// Workflow parameters types
pub mod parameters;
/// Rendering of a configuration into engine command line arguments
pub mod render;
/// Generation of parameters from a workflow's `nextflow_schema.json`
pub mod schema_import;
/// Source of the workflows, e.g. local folder or remote repository
pub mod source;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "database", derive(sea_orm::FromJsonQueryResult))]
pub struct WorkflowConfiguration {
    /// Workflow engine
    engine: engine::Engine,
    /// Workflow engine paramters
    engine_parameters: Vec<engine::EngineParameter>,
    /// Workflow source
    source: source::Source,
    /// Parameters adjustable by the users
    dynamic_parameters: Vec<parameters::Parameter>,
    fixed_parameters: Vec<parameters::Parameter>,
}

impl WorkflowConfiguration {
    pub fn engine(&self) -> &engine::Engine {
        &self.engine
    }

    pub fn engine_mut(&mut self) -> &mut engine::Engine {
        &mut self.engine
    }

    pub fn engine_parameters(&self) -> &[engine::EngineParameter] {
        &self.engine_parameters
    }

    pub fn engine_parameters_mut(&mut self) -> &mut Vec<engine::EngineParameter> {
        &mut self.engine_parameters
    }

    pub fn source(&self) -> &source::Source {
        &self.source
    }

    pub fn source_mut(&mut self) -> &mut source::Source {
        &mut self.source
    }

    pub fn dynamic_parameters(&self) -> &[parameters::Parameter] {
        &self.dynamic_parameters
    }

    pub fn dynamic_parameters_mut(&mut self) -> &mut Vec<parameters::Parameter> {
        &mut self.dynamic_parameters
    }

    pub fn fixed_parameters(&self) -> &[parameters::Parameter] {
        &self.fixed_parameters
    }

    pub fn fixed_parameters_mut(&mut self) -> &mut Vec<parameters::Parameter> {
        &mut self.fixed_parameters
    }
}

impl Default for WorkflowConfiguration {
    fn default() -> Self {
        Self {
            engine: engine::Engine::Nextflow,
            engine_parameters: Vec::new(),
            source: source::Source::Local(source::LocalSource::default()),
            dynamic_parameters: Vec::new(),
            fixed_parameters: Vec::new(),
        }
    }
}
