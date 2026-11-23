//! Image inventory, removal and container events of a runtime driven through its command line,
//! the parts the image garbage collection needs (see `image_store`).

use serde_json::Value;

use super::runtime::{CliRuntime, ContainerRuntime, Error, RuntimeKind};

/// One image of a runtime's store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageInfo {
    /// Full image ID as the runtime reports it (`sha256:...` for Docker, bare hex for Podman)
    pub id: String,
    /// Every name the image has (`repository:tag`), empty for a dangling image
    pub names: Vec<String>,
    /// Size in bytes as the runtime reports it; layers shared with other images are counted in
    /// each of them
    pub size_bytes: u64,
}

impl ImageInfo {
    /// Whether `reference` (an image name or ID as a container was started with) denotes this
    /// image: an exact name, the same name without registry host or `:latest`, or an ID (also a
    /// prefix of it).
    ///
    /// # Arguments
    /// * `reference` - name or ID from a container event
    pub fn matches(&self, reference: &str) -> bool {
        let wanted = normalize_name(reference);
        if self.names.iter().any(|name| normalize_name(name) == wanted) {
            return true;
        }
        let bare = |id: &str| id.strip_prefix("sha256:").unwrap_or(id).to_string();
        let (id, reference) = (bare(&self.id), bare(reference));
        reference.len() >= 12 && id.starts_with(&reference)
    }

    /// The names to untag so that the image is removed (its ID when it has no name)
    pub fn removal_references(&self) -> Vec<String> {
        if self.names.is_empty() {
            vec![self.id.clone()]
        } else {
            self.names.clone()
        }
    }
}

/// Brings an image name into one form: no `docker.io/` or `docker.io/library/` prefix and an
/// explicit tag, so `alpine`, `docker.io/library/alpine` and `alpine:latest` compare equal.
///
/// # Arguments
/// * `name` - image name as written by a user or reported by a runtime
fn normalize_name(name: &str) -> String {
    let name = name
        .strip_prefix("docker.io/")
        .map(|rest| rest.strip_prefix("library/").unwrap_or(rest))
        .unwrap_or(name);
    // A colon after the last slash is a tag, before it a registry port
    let has_tag = name
        .rsplit('/')
        .next()
        .is_some_and(|last| last.contains(':'));
    // Digest references (`name@sha256:...`) stay as they are
    if has_tag || name.contains('@') {
        name.to_string()
    } else {
        format!("{name}:latest")
    }
}

/// Parses the JSON array of `image inspect` (Docker and Podman share the fields used here).
///
/// # Arguments
/// * `json` - output of `<runtime> image inspect <ids>`
pub(crate) fn parse_image_inspect(json: &str) -> Result<Vec<ImageInfo>, serde_json::Error> {
    let value: Value = serde_json::from_str(json)?;
    let entries = value.as_array().cloned().unwrap_or_default();
    Ok(entries
        .iter()
        .filter_map(|entry| {
            let id = entry.get("Id")?.as_str()?.to_string();
            let names = entry
                .get("RepoTags")
                .and_then(Value::as_array)
                .map(|tags| {
                    tags.iter()
                        .filter_map(Value::as_str)
                        // Dangling images are listed under a placeholder
                        .filter(|tag| !tag.contains("<none>"))
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            let size_bytes = entry.get("Size").and_then(Value::as_u64).unwrap_or(0);
            Some(ImageInfo {
                id,
                names,
                size_bytes,
            })
        })
        .collect())
}

/// Image names of the `create` events in `output` (one JSON object per line). Docker names the
/// image in `from` or `Actor.Attributes.image`, Podman in `Image`.
///
/// # Arguments
/// * `output` - stdout of `<runtime> events --format '{{json .}}'`
pub(crate) fn parse_container_events(output: &str) -> Vec<String> {
    let mut images = Vec::new();
    for line in output.lines() {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let status = ["status", "Status", "Action"]
            .iter()
            .find_map(|key| event.get(*key).and_then(Value::as_str))
            .unwrap_or_default();
        if status != "create" {
            continue;
        }
        let image = event
            .get("from")
            .or_else(|| event.get("Image"))
            .or_else(|| event.pointer("/Actor/Attributes/image"))
            .and_then(Value::as_str)
            .filter(|image| !image.is_empty());
        if let Some(image) = image
            && !images.iter().any(|known| known == image)
        {
            images.push(image.to_string());
        }
    }
    images
}

impl CliRuntime {
    /// Lists all images: the IDs from `image ls`, then their details from `image inspect`.
    pub(super) async fn list_images_cli(&self) -> Result<Vec<ImageInfo>, Error> {
        if self.kind() == RuntimeKind::Docker || self.kind() == RuntimeKind::Podman {
            let listing = self
                .run_checked(
                    &["image", "ls", "--all", "--quiet", "--no-trunc"],
                    super::runtime::LIST_TIMEOUT,
                )
                .await?;
            let mut ids: Vec<String> = String::from_utf8_lossy(&listing.stdout)
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
                .collect();
            ids.sort();
            ids.dedup();
            if ids.is_empty() {
                return Ok(Vec::new());
            }
            let mut args = vec!["image".to_string(), "inspect".to_string()];
            args.extend(ids);
            let inspected = self
                .run_checked(&args, super::runtime::LIST_TIMEOUT)
                .await?;
            return parse_image_inspect(&String::from_utf8_lossy(&inspected.stdout)).map_err(
                |source| Error::Parse {
                    binary: self.binary_display(),
                    source,
                },
            );
        }
        Err(Error::Unsupported("listing images"))
    }

    /// Untags the image by every name (by ID if it has none), without `--force`.
    ///
    /// # Arguments
    /// * `image` - image to remove
    pub(super) async fn remove_image_cli(&self, image: ImageInfo) -> Result<(), Error> {
        let mut args = vec!["rmi".to_string()];
        args.extend(image.removal_references());
        self.run_checked(&args, super::runtime::LIST_TIMEOUT)
            .await
            .map(drop)
    }

    /// Reads the container `create` events of the window from the runtime's event log.
    ///
    /// # Arguments
    /// * `since` - window start, seconds since the Unix epoch
    /// * `until` - window end (the command returns when it is reached)
    pub(super) async fn images_used_between_cli(
        &self,
        since: u64,
        until: u64,
    ) -> Result<Vec<String>, Error> {
        if self.kind() != RuntimeKind::Docker && self.kind() != RuntimeKind::Podman {
            return Err(Error::Unsupported("reading container events"));
        }
        let args = [
            "events".to_string(),
            "--since".to_string(),
            since.to_string(),
            "--until".to_string(),
            until.to_string(),
            "--filter".to_string(),
            "type=container".to_string(),
            "--filter".to_string(),
            "event=create".to_string(),
            "--format".to_string(),
            "{{json .}}".to_string(),
        ];
        // The command waits for `until`, which is in the past here, so it returns at once
        let output = self
            .run_checked(&args, super::runtime::LIST_TIMEOUT)
            .await?;
        Ok(parse_container_events(&String::from_utf8_lossy(
            &output.stdout,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Docker and Podman inspect output: names, size, dangling images without a name
    #[test]
    fn inspect_output_is_parsed() {
        let json = r#"[
            {"Id":"sha256:aaa","RepoTags":["alpine:3","alpine:latest"],"Size":7000000},
            {"Id":"sha256:bbb","RepoTags":[],"Size":5},
            {"Id":"sha256:ccc","RepoTags":["<none>:<none>"],"Size":6}
        ]"#;
        let images = parse_image_inspect(json).unwrap();
        assert_eq!(images.len(), 3);
        assert_eq!(images[0].names, ["alpine:3", "alpine:latest"]);
        assert_eq!(images[0].size_bytes, 7_000_000);
        assert!(images[1].names.is_empty());
        assert!(images[2].names.is_empty());
        assert_eq!(images[1].removal_references(), ["sha256:bbb"]);
    }

    /// Only `create` events count; Docker's `from` and Podman's `Image` both work
    #[test]
    fn container_events_name_their_images() {
        let output = r#"{"status":"create","id":"1","from":"alpine:3","Type":"container"}
{"status":"start","id":"1","from":"ignored:1","Type":"container"}
{"Status":"create","Image":"docker.io/library/busybox:1","Type":"container"}
{"status":"create","Actor":{"Attributes":{"image":"alpine:3"}}}
not json
"#;
        assert_eq!(
            parse_container_events(output),
            ["alpine:3", "docker.io/library/busybox:1"]
        );
    }

    /// Names compare without registry host and with an implied `:latest`; IDs by prefix
    #[test]
    fn references_match_images() {
        let image = ImageInfo {
            id: "sha256:0123456789abcdef".into(),
            names: vec!["alpine:latest".into(), "localhost:5000/tool:1".into()],
            size_bytes: 1,
        };
        assert!(image.matches("alpine"));
        assert!(image.matches("docker.io/library/alpine:latest"));
        assert!(image.matches("localhost:5000/tool:1"));
        assert!(!image.matches("localhost:5000/tool"));
        assert!(image.matches("0123456789abcdef"));
        assert!(image.matches("sha256:0123456789ab"));
        assert!(!image.matches("0123"));
        assert!(!image.matches("busybox"));
    }
}
