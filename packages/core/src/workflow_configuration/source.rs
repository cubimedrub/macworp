use std::{fmt::Display, path::PathBuf, str::FromStr};

use serde::{Deserialize, Serialize};
use strum::EnumIter;

use super::engine::Engine;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Unsupported source type `{0}`")]
    UnsupportedSource(String),
}

/// Source of the workflow
/// Local: workflow is on the local filesystem, e.g. for development
/// Remote: workflow is in a remote Git repository, e.g. GitHub, and cloned for execution
/// NfCore: workflow is a nf-core pipeline, e.g. `nf-core/rnaseq`
/// Image: container image to pull (Docker/Podman/wslc)
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, EnumIter)]
pub enum Source {
    Local(LocalSource),
    Remote(RemoteSource),
    NfCore(NfCoreSource),
    /// Container image to pull (Container engine)
    Image(ImageSource),
}

impl Source {
    pub fn is_nf_core(&self) -> bool {
        matches!(self, Source::NfCore(_))
    }

    pub fn is_image(&self) -> bool {
        matches!(self, Source::Image(_))
    }

    /// Whether `engine` can run workflows from this kind of source.
    pub fn is_supported_by(&self, engine: &Engine) -> bool {
        engine
            .supported_sources()
            .iter()
            .any(|source| source.is_same_variant(self))
    }

    pub fn is_local(&self) -> bool {
        matches!(self, Source::Local(_))
    }

    pub fn is_remote(&self) -> bool {
        matches!(self, Source::Remote(_))
    }

    pub fn is_same_variant(&self, other: &Source) -> bool {
        std::mem::discriminant(self) == std::mem::discriminant(other)
    }
}

impl Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Local(_) => write!(f, "Local"),
            Source::Remote(_) => write!(f, "Remote"),
            Source::NfCore(_) => write!(f, "nf-core"),
            Source::Image(_) => write!(f, "Image"),
        }
    }
}

impl FromStr for Source {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "local" => Ok(Source::Local(LocalSource::default())),
            "remote" => Ok(Source::Remote(RemoteSource::default())),
            "nfcore" | "nf-core" => Ok(Source::NfCore(NfCoreSource::default())),
            "image" => Ok(Source::Image(ImageSource::default())),
            _ => Err(Error::UnsupportedSource(s.to_string())),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LocalSource {
    /// Location to the folder containing the workflow
    location: PathBuf,
    /// Path to the main script within location
    script: PathBuf,
}

impl LocalSource {
    pub fn new(location: PathBuf, script: PathBuf) -> Self {
        Self { location, script }
    }

    pub fn location(&self) -> &PathBuf {
        &self.location
    }

    pub fn location_mut(&mut self) -> &mut PathBuf {
        &mut self.location
    }

    pub fn script(&self) -> &PathBuf {
        &self.script
    }

    pub fn script_mut(&mut self) -> &mut PathBuf {
        &mut self.script
    }
}

impl Default for LocalSource {
    fn default() -> Self {
        Self {
            location: PathBuf::from("/tmp/my-workflow"),
            script: PathBuf::from("main.nf"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RemoteSource {
    /// Url of the GitHub repository
    url: String,
    /// Optional git tag, branch or commit hash to checkout. Default depends on workfloe engine
    version: Option<String>,
    /// Path to the main-script. Default depends on workfloe engine
    script: Option<PathBuf>,
}

impl RemoteSource {
    pub fn new(url: String, version: Option<String>, script: Option<PathBuf>) -> Self {
        Self {
            url,
            version,
            script,
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    pub fn url_mut(&mut self) -> &mut String {
        &mut self.url
    }

    pub fn version(&self) -> Option<&String> {
        self.version.as_ref()
    }

    pub fn version_mut(&mut self) -> &mut Option<String> {
        &mut self.version
    }

    pub fn script(&self) -> Option<&PathBuf> {
        self.script.as_ref()
    }

    pub fn script_mut(&mut self) -> &mut Option<PathBuf> {
        &mut self.script
    }
}

impl Default for RemoteSource {
    fn default() -> Self {
        Self {
            url: String::new(),
            version: None,
            script: Some(PathBuf::from("main.nf")),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct NfCoreSource {
    /// Name of the nf-core pipeline, e.g. `rnaseq`
    pipeline: String,
    /// Optional pipeline release/tag (`-r`). Latest if unset
    version: Option<String>,
}

impl NfCoreSource {
    pub fn new(pipeline: String, version: Option<String>) -> Self {
        Self { pipeline, version }
    }

    pub fn pipeline(&self) -> &str {
        &self.pipeline
    }

    pub fn pipeline_mut(&mut self) -> &mut String {
        &mut self.pipeline
    }

    pub fn version(&self) -> Option<&String> {
        self.version.as_ref()
    }

    pub fn version_mut(&mut self) -> &mut Option<String> {
        &mut self.version
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ImageSource {
    /// Image reference to pull, e.g. `docker.io/library/hello-world:latest`
    image: String,
}

impl ImageSource {
    pub fn new(image: String) -> Self {
        Self { image }
    }

    pub fn image(&self) -> &str {
        &self.image
    }

    pub fn image_mut(&mut self) -> &mut String {
        &mut self.image
    }
}

#[cfg(test)]
mod tests {
    use strum::IntoEnumIterator;

    use super::*;

    #[test]
    fn display_from_str_roundtrip() {
        for source in Source::iter() {
            assert!(
                source
                    .to_string()
                    .parse::<Source>()
                    .unwrap()
                    .is_same_variant(&source)
            );
        }
        assert!("nf-core".parse::<Source>().unwrap().is_nf_core());
        assert_eq!(Source::iter().count(), 4);
        assert_eq!(
            Source::NfCore(NfCoreSource::default()).to_string(),
            "nf-core"
        );
    }

    #[test]
    fn nf_core_serde_roundtrip() {
        let source = Source::NfCore(NfCoreSource::new("rnaseq".into(), Some("3.14.0".into())));
        let json = serde_json::to_string(&source).unwrap();
        assert_eq!(serde_json::from_str::<Source>(&json).unwrap(), source);
    }

    #[test]
    fn sources_are_engine_specific() {
        let nf_core = Source::NfCore(NfCoreSource::default());
        let local = Source::Local(LocalSource::default());
        assert!(nf_core.is_supported_by(&Engine::Nextflow));
        assert!(!nf_core.is_supported_by(&Engine::Snakemake));
        assert!(!nf_core.is_supported_by(&Engine::Cwl));
        assert!(local.is_supported_by(&Engine::Cwl));
        assert!(!local.is_supported_by(&Engine::Container));
        assert!(Source::Image(ImageSource::default()).is_supported_by(&Engine::Container));
    }
}
