use std::{fmt::Display, str::FromStr};

use serde::{Deserialize, Serialize};
use strum::EnumIter;
use thiserror::Error;

use super::source::{ImageSource, LocalSource, NfCoreSource, RemoteSource, Source};

#[derive(Debug, Error)]
pub enum Error {
    #[error("Workflow engine `{0}` is not supported")]
    UnsupportedEngine(String),
}

/// Supported workflow engines.
/// * Nextflow: https://www.nextflow.io/
/// * Snakemake: https://snakemake.readthedocs.io/en/stable
/// * CWL: https://www.commonwl.org/
/// * Container: run a single OCI image (technically not a workflow engine but handy for containerized tools).
///   Docker/Podman/wslc is a host/worker setting, not part of the workflow definition.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, EnumIter)]
pub enum Engine {
    Nextflow,
    Snakemake,
    Cwl,
    Container,
}

/// Versions of one engine that have a prebuilt engine image, see `docker/engine-versions.yml`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EngineCatalog {
    /// Name of the build target in `docker/engine.dockerfile`
    pub target: String,
    /// Build argument of the dockerfile that selects the version
    pub build_arg: String,
    /// Version of the image workflows run on when they pin nothing
    pub default: String,
    /// All versions with an image, a workflow can pin any of them
    pub versions: Vec<String>,
}

/// The catalog file, shared with the CI job that builds the images.
const CATALOG: &str = include_str!("../../../../docker/engine-versions.yml");

impl Engine {
    /// The versions with a prebuilt image, `None` for engines without images of their own
    /// (`Container` runs the workflow's image).
    pub fn catalog(&self) -> Option<&'static EngineCatalog> {
        use std::{collections::HashMap, sync::OnceLock};
        static CATALOGS: OnceLock<HashMap<String, EngineCatalog>> = OnceLock::new();
        let catalogs = CATALOGS.get_or_init(|| {
            serde_norway::from_str(CATALOG).expect("docker/engine-versions.yml is valid")
        });
        let key = match self {
            Engine::Nextflow => "nextflow",
            Engine::Snakemake => "snakemake",
            Engine::Cwl => "cwl",
            Engine::Container => return None,
        };
        catalogs.get(key)
    }

    /// Name of the engine image without registry and tag (`macworp-nextflow`), `None` for
    /// engines without images of their own.
    pub fn image_repository(&self) -> Option<String> {
        self.catalog()
            .map(|catalog| format!("macworp-{}", catalog.target))
    }

    /// Every engine, in declaration order.
    pub fn all() -> Vec<Engine> {
        use strum::IntoEnumIterator;
        Engine::iter().collect()
    }

    /// Name of the engine parameter that continues an interrupted run in the same project
    /// folder, `None` if the engine needs none or cannot resume.
    ///
    /// * Nextflow: `-resume` reuses cached tasks.
    /// * CWL (Toil): none. `--restart` would continue the stored job graph with its stored
    ///   inputs, ignoring corrected values. The worker starts afresh with a shared step cache
    ///   instead (`--cachedir`), which skips the steps whose inputs did not change.
    /// * Snakemake: reruns only what is missing or outdated by itself, no flag needed.
    /// * Container: a single image, nothing to resume.
    pub fn resume_parameter(&self) -> Option<&'static str> {
        match self {
            Engine::Nextflow => Some("resume"),
            Engine::Cwl | Engine::Snakemake | Engine::Container => None,
        }
    }

    /// Conventional main script file name of this engine, `None` if it has none (CWL files and
    /// container images are named freely).
    pub fn default_script(&self) -> Option<&'static str> {
        match self {
            Engine::Nextflow => Some("main.nf"),
            Engine::Snakemake => Some("Snakefile"),
            Engine::Cwl | Engine::Container => None,
        }
    }

    /// Source kinds this engine can run, each with default payload.
    pub fn supported_sources(&self) -> Vec<Source> {
        let local = Source::Local(LocalSource::default());
        let remote = Source::Remote(RemoteSource::default());
        match self {
            Engine::Nextflow => vec![local, remote, Source::NfCore(NfCoreSource::default())],
            Engine::Snakemake | Engine::Cwl => vec![local, remote],
            Engine::Container => vec![Source::Image(ImageSource::default())],
        }
    }
}

impl Display for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Engine::Nextflow => write!(f, "Nextflow"),
            Engine::Snakemake => write!(f, "Snakemake"),
            Engine::Cwl => write!(f, "CWL"),
            Engine::Container => write!(f, "Container"),
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
            "container" => Ok(Engine::Container),
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

#[cfg(test)]
/// Tests of the engine catalog.
mod catalog_tests {
    use super::*;

    /// Every engine with images lists its default among its versions, `Container` has none.
    #[test]
    fn catalog_is_consistent() {
        for engine in Engine::all() {
            match engine.catalog() {
                Some(catalog) => {
                    assert!(catalog.versions.contains(&catalog.default), "{engine}");
                    for version in &catalog.versions {
                        assert!(
                            crate::workflow_configuration::validate_engine_version(version).is_ok()
                        );
                    }
                }
                None => assert_eq!(engine, Engine::Container),
            }
        }
        // The newest version is listed first and is the default
        for engine in [Engine::Nextflow, Engine::Snakemake, Engine::Cwl] {
            let catalog = engine.catalog().unwrap();
            assert_eq!(catalog.versions.first(), Some(&catalog.default), "{engine}");
        }
        assert_eq!(
            Engine::Nextflow.image_repository().as_deref(),
            Some("macworp-nextflow")
        );
    }
}
