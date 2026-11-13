use dioxus::prelude::*;
use macworp_core::responses::{NfCorePipeline, NfCoreRelease};

use crate::{configuration::Configuration, session::Session, storage::IsStorage};

use super::super::Spinner;

/// Pipeline name without whitespace and the `nf-core/` prefix, as the backend looks it up.
///
/// # Arguments
/// * `pipeline` - Name as typed by the user
pub fn normalized_pipeline(pipeline: &str) -> &str {
    let pipeline = pipeline.trim();
    pipeline.strip_prefix("nf-core/").unwrap_or(pipeline)
}

/// Text of a pipeline suggestion: `<name> - <description>` (archived pipelines are marked).
///
/// # Arguments
/// * `pipeline` - Pipeline of the nf-core index
pub fn suggestion_label(pipeline: &NfCorePipeline) -> String {
    let archived = if pipeline.archived { "(archived) " } else { "" };
    if pipeline.description.is_empty() && !pipeline.archived {
        return pipeline.name.clone();
    }
    format!("{} - {archived}{}", pipeline.name, pipeline.description)
        .trim_end()
        .to_string()
}

/// Release tags to offer in the version select: the releases (newest first) and the current
/// version at the end if it is not a release (e.g. a branch), so opening the editor never
/// changes the saved value.
///
/// # Arguments
/// * `releases` - Releases of the pipeline
/// * `current` - Version saved in the workflow, `None` for the latest
pub fn release_options(releases: &[NfCoreRelease], current: Option<&str>) -> Vec<String> {
    let mut options: Vec<String> = releases.iter().map(|release| release.tag.clone()).collect();
    if let Some(current) = current.filter(|current| !current.is_empty()) {
        if !options.iter().any(|option| option == current) {
            options.push(current.to_string());
        }
    }
    options
}

#[derive(Props, Clone, PartialEq)]
pub struct NfCoreSourceFieldsProps {
    /// Pipeline of the workflow, with or without the `nf-core/` prefix
    pub pipeline: String,
    /// Version of the workflow, `None` for the latest
    pub version: Option<String>,
    /// Called with the new pipeline name
    pub on_pipeline: EventHandler<String>,
    /// Called with the new version
    pub on_version: EventHandler<Option<String>>,
}

/// Pipeline and version inputs of the nf-core source: the pipeline is free text with the pipelines
/// of the nf-core index as suggestions, the version is a select of the releases of the pipeline.
/// Both fall back to plain text when the index is unavailable or does not know the pipeline
/// (private forks, offline).
#[component]
pub fn NfCoreSourceFields<S: IsStorage + 'static>(props: NfCoreSourceFieldsProps) -> Element {
    let storage = use_context::<Signal<S>>();
    let config = use_context::<Signal<Configuration>>();

    let pipelines = use_resource(move || async move {
        let session = Session::new(storage, config)?;
        session
            .client
            .utilities()
            .nf_core_pipelines(&session.jwt)
            .await
            .map_err(|e| e.to_string())
    });

    let name = normalized_pipeline(&props.pipeline).to_string();
    let releases = use_resource(use_reactive!(|name| async move {
        if name.is_empty() {
            return Err("No pipeline".to_string());
        }
        let session = Session::new(storage, config)?;
        session
            .client
            .utilities()
            .nf_core_releases(&session.jwt, &name)
            .await
            .map_err(|e| e.to_string())
    }));

    let version = props.version.clone();
    let on_version = props.on_version;
    let on_pipeline = props.on_pipeline;
    let release_select = match &*releases.read() {
        None => rsx! { Spinner {} },
        Some(Ok(response)) => {
            let options = release_options(&response.releases, version.as_deref());
            let selected = version.clone().unwrap_or_default();
            rsx! {
                select {
                    class: "form-select",
                    aria_label: "nf-core pipeline version",
                    aria_describedby: "workflow-integration-configuration-source-nfcore-version",
                    value: "{selected}",
                    onchange: move |event| {
                        let value = event.value();
                        on_version.call(if value.is_empty() { None } else { Some(value) });
                    },
                    option { value: "", selected: selected.is_empty(), "Latest" }
                    for option in options {
                        option { value: "{option}", selected: option == selected, "{option}" }
                    }
                }
            }
        }
        Some(Err(_)) => rsx! {
            input {
                r#type: "text",
                class: "form-control",
                placeholder: "3.14.0 (empty = latest)",
                aria_label: "nf-core pipeline version",
                aria_describedby: "workflow-integration-configuration-source-nfcore-version",
                value: "{version.clone().unwrap_or_default()}",
                oninput: move |event| {
                    let value = event.value();
                    on_version.call(if value.is_empty() { None } else { Some(value) });
                }
            }
        },
    };

    let pipelines_read = pipelines.read();
    let (suggestions, stale) = match &*pipelines_read {
        Some(Ok(response)) => (response.pipelines.clone(), response.stale),
        _ => (Vec::new(), false),
    };

    rsx! {
        div {
            class: "input-group mb-3",
            span {
                class: "input-group-text",
                id: "workflow-integration-configuration-source-nfcore-pipeline",
                "Pipeline"
            }
            input {
                r#type: "text",
                class: "form-control",
                list: "workflow-integration-nfcore-pipelines",
                placeholder: "rnaseq",
                aria_label: "nf-core pipeline name",
                aria_describedby: "workflow-integration-configuration-source-nfcore-pipeline",
                value: "{props.pipeline}",
                oninput: move |event| on_pipeline.call(event.value())
            }
            datalist {
                id: "workflow-integration-nfcore-pipelines",
                for pipeline in suggestions {
                    option {
                        value: "{pipeline.name}",
                        label: suggestion_label(&pipeline)
                    }
                }
            }
        }
        div {
            class: "input-group mb-3",
            span {
                class: "input-group-text",
                id: "workflow-integration-configuration-source-nfcore-version",
                "Version"
            }
            {release_select}
        }
        if stale {
            p {
                class: "form-text",
                "The nf-core index could not be refreshed, the list may be outdated."
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str) -> NfCoreRelease {
        NfCoreRelease {
            tag: tag.to_string(),
            published_at: String::new(),
            has_schema: true,
        }
    }

    #[test]
    fn suggestion_shows_name_then_description() {
        let mut pipeline = NfCorePipeline {
            name: "rnaseq".to_string(),
            description: "RNA sequencing".to_string(),
            archived: false,
        };
        assert_eq!(suggestion_label(&pipeline), "rnaseq - RNA sequencing");
        pipeline.archived = true;
        assert_eq!(
            suggestion_label(&pipeline),
            "rnaseq - (archived) RNA sequencing"
        );
        pipeline.description.clear();
        assert_eq!(suggestion_label(&pipeline), "rnaseq - (archived)");
        pipeline.archived = false;
        assert_eq!(suggestion_label(&pipeline), "rnaseq");
    }

    #[test]
    fn prefix_and_whitespace_are_removed() {
        assert_eq!(normalized_pipeline(" nf-core/rnaseq "), "rnaseq");
        assert_eq!(normalized_pipeline("rnaseq"), "rnaseq");
        assert_eq!(normalized_pipeline("myfork/rnaseq"), "myfork/rnaseq");
        assert_eq!(normalized_pipeline(""), "");
    }

    #[test]
    fn current_version_outside_the_releases_is_kept() {
        let releases = [release("2.0"), release("1.0")];
        assert_eq!(release_options(&releases, None), ["2.0", "1.0"]);
        assert_eq!(release_options(&releases, Some("1.0")), ["2.0", "1.0"]);
        assert_eq!(
            release_options(&releases, Some("dev")),
            ["2.0", "1.0", "dev"]
        );
        assert_eq!(release_options(&releases, Some("")), ["2.0", "1.0"]);
    }
}
