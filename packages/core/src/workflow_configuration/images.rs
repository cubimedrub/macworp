//! Container images a workflow builds from its own `Dockerfile`s, see ADR 0007. The maintainer
//! maps each build file to the exact image name the workflow uses; the worker builds them before
//! the engine starts.

use std::path::{Component, Path};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Longest accepted image name including the tag (the limit of container runtimes).
pub const IMAGE_NAME_MAX_LENGTH: usize = 255;

/// Longest accepted tag.
const TAG_MAX_LENGTH: usize = 128;

/// One image built from a build file of the workflow.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageBuild {
    /// Build file relative to the workflow directory; its folder is the build context.
    dockerfile: String,
    /// Name the workflow uses for the image, see [`validate_image_name`].
    name: String,
}

impl ImageBuild {
    pub fn new(dockerfile: String, name: String) -> Self {
        Self { dockerfile, name }
    }

    pub fn dockerfile(&self) -> &str {
        &self.dockerfile
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Why the image builds of a workflow are rejected.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum Error {
    /// The image name breaks the name rule (name, reason)
    #[error("Invalid image name `{0}`: {1}")]
    InvalidName(String, &'static str),
    /// The build file path is empty, absolute or leaves the workflow directory
    #[error("Build file `{0}` must be a path inside the workflow directory")]
    InvalidDockerfile(String),
    /// Two entries use the same image name
    #[error("Image name `{0}` is used more than once")]
    DuplicateName(String),
    /// Builds are only supported for local workflows
    #[error("Image builds are only supported for local workflows")]
    LocalOnly,
    /// Two entries use the same build file
    #[error("Build file `{0}` is used more than once")]
    DuplicateDockerfile(String),
}

/// Checks that `name` is a locally built image name of the form `<prefix>/<name>:<version>`:
/// at least one `/`, an explicit tag that is not `latest`, no `library/` prefix, and a first
/// component that is not a registry host (contains `.` or `:`, or is `localhost`). This keeps
/// the names of official and registry images (`debian:12`, `ghcr.io/org/tool:1`) from being
/// mapped to a build.
///
/// # Arguments
/// * `name` - the image name as entered by the workflow maintainer
pub fn validate_image_name(name: &str) -> Result<(), Error> {
    let invalid = |reason| Error::InvalidName(name.to_string(), reason);
    if name.is_empty() || name.len() > IMAGE_NAME_MAX_LENGTH {
        return Err(invalid("must have 1 to 255 characters"));
    }
    let (repository, tag) = match name.rsplit_once('/') {
        Some((head, last)) => match last.split_once(':') {
            Some((last, tag)) => (format!("{head}/{last}"), Some(tag)),
            None => (name.to_string(), None),
        },
        None => return Err(invalid("needs a prefix, use `<prefix>/<name>:<version>`")),
    };
    if name.contains('@') {
        return Err(invalid("digests are not supported, use a tag"));
    }
    let Some(tag) = tag else {
        return Err(invalid(
            "needs an explicit tag, use `<prefix>/<name>:<version>`",
        ));
    };
    let components: Vec<&str> = repository.split('/').collect();
    let first = components[0];
    if first == "localhost" || first.contains(['.', ':']) {
        return Err(invalid("must not start with a registry host"));
    }
    if first == "library" {
        return Err(invalid("`library/` is reserved for official images"));
    }
    if !components.iter().all(|c| valid_component(c)) {
        return Err(invalid(
            "only lowercase letters and digits, separated by `.`, `_`, `__` or `-`, per `/` part",
        ));
    }
    if tag == "latest" {
        return Err(invalid("the tag `latest` is not allowed, use a version"));
    }
    if !valid_tag(tag) {
        return Err(invalid(
            "the tag may only contain letters, digits, `_`, `.` and `-` and must not start with `.` or `-`",
        ));
    }
    Ok(())
}

/// One `/`-separated part of the repository: lowercase alphanumeric runs joined by `.`, `_`,
/// `__` or runs of `-`.
fn valid_component(component: &str) -> bool {
    let alphanumeric = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    let starts_and_ends = component.chars().next().is_some_and(alphanumeric)
        && component.chars().last().is_some_and(alphanumeric);
    starts_and_ends
        && component
            .chars()
            .all(|c| alphanumeric(c) || matches!(c, '.' | '_' | '-'))
        && !component.contains("...")
        && !component.contains("___")
        && !component.contains("._")
        && !component.contains("_.")
        && !component.contains("..")
}

/// A tag: letters, digits, `_`, `.`, `-`, up to 128 characters, not starting with `.` or `-`.
fn valid_tag(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= TAG_MAX_LENGTH
        && !tag.starts_with(['.', '-'])
        && tag
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// Checks that `dockerfile` is a relative path that stays inside the workflow directory:
/// not empty, no root, drive prefix or `..`, no backslash.
///
/// # Arguments
/// * `dockerfile` - the build file path as stored in the configuration
pub fn validate_dockerfile(dockerfile: &str) -> Result<(), Error> {
    let path = Path::new(dockerfile);
    let confined = path
        .components()
        .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    let names_a_file = path.components().any(|c| matches!(c, Component::Normal(_)));
    if confined && names_a_file && !dockerfile.contains(['\\', '\0']) {
        Ok(())
    } else {
        Err(Error::InvalidDockerfile(dockerfile.to_string()))
    }
}

/// Checks every build: valid name and build file, no name and no file used twice.
///
/// # Arguments
/// * `images` - the builds of one workflow configuration
pub fn validate_images(images: &[ImageBuild]) -> Result<(), Error> {
    let mut names = std::collections::HashSet::new();
    let mut files = std::collections::HashSet::new();
    for image in images {
        validate_image_name(&image.name)?;
        validate_dockerfile(&image.dockerfile)?;
        if !names.insert(image.name.as_str()) {
            return Err(Error::DuplicateName(image.name.clone()));
        }
        // `./a/Dockerfile` and `a/Dockerfile` are the same file
        let normal = crate::paths::sanitize_relative(Path::new(&image.dockerfile));
        if !files.insert(normal) {
            return Err(Error::DuplicateDockerfile(image.dockerfile.clone()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(dockerfile: &str, name: &str) -> ImageBuild {
        ImageBuild::new(dockerfile.into(), name.into())
    }

    #[test]
    fn accepts_prefixed_versioned_names() {
        for name in [
            "a/b:1",
            "myteam/tool:1.2",
            "myteam/sub/tool:1.2-rc_1",
            "my-team/to_ol:v1",
        ] {
            assert_eq!(validate_image_name(name), Ok(()), "{name}");
        }
    }

    #[test]
    fn rejects_official_registry_and_unversioned_names() {
        for name in [
            "debian:12-slim",
            "debian",
            "library/debian:12",
            "ghcr.io/o/t:1",
            "localhost/a:1",
            "localhost:5000/a:1",
            "host:5000/a/b:1",
            "a/b",
            "a/b:",
            "a/b:latest",
            "a/b:-x",
            "a/b:x y",
            "a/b@sha256:abcd",
            "A/b:1",
            "a//b:1",
            "/a/b:1",
            "a/b/:1",
            "a/-b:1",
            "a/b..c:1",
            "",
        ] {
            assert!(validate_image_name(name).is_err(), "{name}");
        }
        let long = format!("a/{}:1", "b".repeat(IMAGE_NAME_MAX_LENGTH));
        assert!(validate_image_name(&long).is_err());
    }

    #[test]
    fn dockerfile_must_stay_inside_the_workflow() {
        for file in ["Dockerfile", "docker/Dockerfile.tool", "./a/b.dockerfile"] {
            assert_eq!(validate_dockerfile(file), Ok(()), "{file}");
        }
        for file in [
            "",
            ".",
            "/etc/Dockerfile",
            "../Dockerfile",
            "a/../../Dockerfile",
            "a\\..\\b",
            "a\0b",
        ] {
            assert!(validate_dockerfile(file).is_err(), "{file:?}");
        }
    }

    #[test]
    fn duplicates_are_rejected() {
        assert_eq!(
            validate_images(&[
                build("a/Dockerfile", "t/a:1"),
                build("b/Dockerfile", "t/a:1")
            ]),
            Err(Error::DuplicateName("t/a:1".into()))
        );
        assert_eq!(
            validate_images(&[
                build("a/Dockerfile", "t/a:1"),
                build("./a/Dockerfile", "t/b:1")
            ]),
            Err(Error::DuplicateDockerfile("./a/Dockerfile".into()))
        );
        assert_eq!(
            validate_images(&[
                build("a/Dockerfile", "t/a:1"),
                build("b/Dockerfile", "t/a:2")
            ]),
            Ok(())
        );
    }
}
