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
    /// Delete the engine's work files (Nextflow `work/`, CWL `.toil/`) after a run succeeded.
    /// Failed and cancelled runs keep them so they can be resumed. Absent in configurations
    /// stored before this field existed, which means `false`.
    #[serde(default)]
    cleanup_on_success: bool,
}

impl WorkflowConfiguration {
    /// Whether the engine's work files are deleted after a successful run.
    pub fn cleanup_on_success(&self) -> bool {
        self.cleanup_on_success
    }

    /// Mutable access to the cleanup switch, see [`Self::cleanup_on_success`].
    pub fn cleanup_on_success_mut(&mut self) -> &mut bool {
        &mut self.cleanup_on_success
    }

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

    /// Adds the engine's resume parameter ([`engine::Engine::resume_parameter`]) unless the
    /// workflow already sets it. Does nothing for engines without one.
    pub fn enable_resume(&mut self) {
        let Some(name) = self.engine.resume_parameter() else {
            return;
        };
        if self
            .engine_parameters
            .iter()
            .any(|parameter| parameter.name() == name)
        {
            return;
        }
        self.engine_parameters
            .push(engine::EngineParameter::new(name.into(), String::new()));
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

    /// Checks the file/folder patterns of every `Path` parameter (fixed and dynamic).
    ///
    /// Returns a message naming the first parameter with a rejected pattern.
    pub fn validate_path_patterns(&self) -> Result<(), String> {
        for parameter in self.fixed_parameters.iter().chain(&self.dynamic_parameters) {
            if let parameters::Parameter::Path(path) = parameter {
                use parameters::parameter::IsParameter;
                path.validate_patterns()
                    .map_err(|e| format!("Parameter `{}`: {e}", path.name()))?;
            }
        }
        Ok(())
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
            cleanup_on_success: false,
        }
    }
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;

    /// Configurations stored before the switch existed still load, with cleanup off.
    #[test]
    fn missing_cleanup_switch_means_off() {
        let mut value = serde_json::to_value(WorkflowConfiguration::default()).unwrap();
        value.as_object_mut().unwrap().remove("cleanup_on_success");
        let configuration: WorkflowConfiguration = serde_json::from_value(value).unwrap();
        assert!(!configuration.cleanup_on_success());
    }

    /// The switch survives a serialization round trip.
    #[test]
    fn cleanup_switch_round_trips() {
        let mut configuration = WorkflowConfiguration::default();
        *configuration.cleanup_on_success_mut() = true;
        let json = serde_json::to_string(&configuration).unwrap();
        let loaded: WorkflowConfiguration = serde_json::from_str(&json).unwrap();
        assert!(loaded.cleanup_on_success());
    }
}

#[cfg(test)]
mod resume_tests {
    use super::*;

    /// Nextflow and CWL get their resume parameter once, Snakemake and Container none.
    #[test]
    fn enable_resume_adds_the_engine_parameter_once() {
        for (engine, expected) in [
            (engine::Engine::Nextflow, vec!["resume"]),
            (engine::Engine::Cwl, vec!["restart"]),
            (engine::Engine::Snakemake, vec![]),
            (engine::Engine::Container, vec![]),
        ] {
            let mut configuration = WorkflowConfiguration::default();
            *configuration.engine_mut() = engine;
            configuration.enable_resume();
            configuration.enable_resume();
            let names: Vec<&str> = configuration
                .engine_parameters()
                .iter()
                .map(|parameter| parameter.name())
                .collect();
            assert_eq!(names, expected);
        }
    }

    /// A workflow that already sets the parameter (possibly with a value) keeps it unchanged.
    #[test]
    fn enable_resume_keeps_an_existing_parameter() {
        let mut configuration = WorkflowConfiguration::default();
        configuration
            .engine_parameters_mut()
            .push(engine::EngineParameter::new("resume".into(), "abc".into()));
        configuration.enable_resume();
        assert_eq!(configuration.engine_parameters().len(), 1);
        assert_eq!(configuration.engine_parameters()[0].value(), "abc");
    }
}
