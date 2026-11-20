use serde::{Deserialize, Serialize};

/// Enum for workflow engines, e.g. Nextflow, Snakemake, CWL, Container
pub mod engine;
/// Allowlist of the engine parameters a workflow may set, with value rules
pub mod engine_parameters;
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
    /// Version of the workflow engine to run, the tag of its engine image (for example
    /// `25.04.6` for `macworp-nextflow:25.04.6`). `None` runs the worker's default image.
    /// Absent in configurations stored before this field existed. Ignored by `Container`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    engine_version: Option<String>,
}

/// Longest accepted engine version (container image tags are limited to 128 characters).
pub const ENGINE_VERSION_MAX_LENGTH: usize = 128;

/// Checks that `version` is usable as an image tag: 1 to [`ENGINE_VERSION_MAX_LENGTH`] ASCII
/// letters, digits, `_`, `.` or `-`, not starting with `.` or `-` (it must not look like an
/// option or a path when handed to the container runtime).
///
/// # Arguments
/// * `version` - the engine version as entered by the workflow author
pub fn validate_engine_version(version: &str) -> Result<(), String> {
    let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-');
    if version.is_empty() || version.len() > ENGINE_VERSION_MAX_LENGTH {
        return Err(format!(
            "Engine version must have 1 to {ENGINE_VERSION_MAX_LENGTH} characters"
        ));
    }
    if version.starts_with(['.', '-']) || !version.chars().all(allowed) {
        return Err(
            "Engine version may only contain letters, digits, `_`, `.` and `-` and must not start with `.` or `-`"
                .into(),
        );
    }
    Ok(())
}

impl WorkflowConfiguration {
    /// Whether the engine's work files are deleted after a successful run.
    pub fn cleanup_on_success(&self) -> bool {
        self.cleanup_on_success
    }

    /// The pinned engine version, `None` for the worker's default, see [`validate_engine_version`].
    pub fn engine_version(&self) -> Option<&str> {
        self.engine_version.as_deref()
    }

    /// Mutable access to the pinned engine version, see [`Self::engine_version`].
    pub fn engine_version_mut(&mut self) -> &mut Option<String> {
        &mut self.engine_version
    }

    /// Checks the pinned engine version, if any: it must be a valid tag
    /// ([`validate_engine_version`]) and one of the versions with a prebuilt image
    /// ([`engine::Engine::catalog`]). `Container` workflows cannot pin a version.
    pub fn validate_engine_version(&self) -> Result<(), String> {
        let Some(version) = self.engine_version.as_deref() else {
            return Ok(());
        };
        validate_engine_version(version)?;
        match self.engine.catalog() {
            Some(catalog) if catalog.versions.iter().any(|known| known == version) => Ok(()),
            Some(catalog) => Err(format!(
                "Engine version `{version}` is not available, choose one of: {}",
                catalog.versions.join(", ")
            )),
            None => Err(format!("The {} engine has no versions to pin", self.engine)),
        }
    }

    /// Checks every engine parameter against the engine's allowlist
    /// ([`engine::Engine::check_parameter`]) and rejects duplicates. Run on the stored
    /// configuration, before the backend adds the engine's resume parameter for a retry.
    pub fn validate_engine_parameters(&self) -> Result<(), engine_parameters::Error> {
        let mut seen = std::collections::HashSet::new();
        for parameter in &self.engine_parameters {
            self.engine
                .check_parameter(parameter.name(), parameter.value())?;
            if !seen.insert(parameter.name()) {
                return Err(engine_parameters::Error::Duplicate(
                    parameter.name().to_string(),
                ));
            }
        }
        Ok(())
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
            engine_version: None,
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

    /// Nextflow gets its resume parameter once, CWL (step cache), Snakemake and Container none.
    #[test]
    fn enable_resume_adds_the_engine_parameter_once() {
        for (engine, expected) in [
            (engine::Engine::Nextflow, vec!["resume"]),
            (engine::Engine::Cwl, vec![]),
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

#[cfg(test)]
/// Tests of the pinned engine version.
mod engine_version_tests {
    use super::*;

    /// Plain version strings are valid tags.
    #[test]
    fn accepts_image_tags() {
        for version in ["25.04.6", "9.11.4", "latest", "v1_2-rc.1"] {
            assert!(validate_engine_version(version).is_ok(), "{version}");
        }
    }

    /// Options, paths, separators and over-long values are not.
    #[test]
    fn rejects_anything_else() {
        for version in ["", "-v", ".x", "a b", "a:b", "a/b", "--privileged", "é"] {
            assert!(validate_engine_version(version).is_err(), "{version}");
        }
        assert!(validate_engine_version(&"a".repeat(ENGINE_VERSION_MAX_LENGTH + 1)).is_err());
    }

    /// Only versions with an image can be pinned, and not on engines without images.
    #[test]
    fn pinned_version_must_be_in_the_catalog() {
        let mut configuration = WorkflowConfiguration::default();
        let known = configuration.engine().catalog().unwrap().default.clone();
        *configuration.engine_version_mut() = Some(known);
        assert!(configuration.validate_engine_version().is_ok());
        *configuration.engine_version_mut() = Some("0.0.1".into());
        assert!(configuration.validate_engine_version().is_err());
        *configuration.engine_mut() = engine::Engine::Container;
        *configuration.engine_version_mut() = Some("25.04.6".into());
        assert!(configuration.validate_engine_version().is_err());
    }

    /// Configurations stored before the field existed still load, unpinned.
    #[test]
    fn missing_version_means_default() {
        let value = serde_json::to_value(WorkflowConfiguration::default()).unwrap();
        assert!(value.get("engine_version").is_none());
        let configuration: WorkflowConfiguration = serde_json::from_value(value).unwrap();
        assert_eq!(configuration.engine_version(), None);
    }
}
