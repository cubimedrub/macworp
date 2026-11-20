use std::{path::PathBuf, str::FromStr, time::Duration};

use dioxus::prelude::*;
use macworp_core::{
    entities::Role,
    responses::{PrefetchResponse, WorkflowIntegrationResponse},
    workflow_configuration::{
        engine::{Engine, EngineParameter},
        parameters::Parameter,
        schema_import::{
            parameters_from_cwl, parameters_from_nextflow_schema, parameters_from_snakemake_schema,
            Imported,
        },
        source::Source,
    },
};
use strum::IntoEnumIterator;
use thiserror::Error;

use crate::{
    client::{Client, ShareKind},
    components::{
        separator::Separator, sharing::share_dialog::ShareDialog, workflow_integrations::*,
        ConfirmationDialog, OwnerlessBadge, Spinner,
    },
    configuration::Configuration,
    current_user::CurrentUser,
    local_workflow::LocalFolderPicker,
    parameter_arrangement::{self, ParameterMove},
    router::IsRouter,
    runtime_status::{engine_hint, ExecutionEnvironmentNote},
    sharing::SharingEnabled,
    storage::IsStorage,
    timer::sleep,
};

#[derive(Debug, Error)]
pub enum Error<S: IsStorage> {
    #[error("Config not loaded")]
    ConfigNotLoaded,
    #[error("Failed to fetch project: {0}")]
    FetchProject(crate::client::Error),
    #[error("Failed to delete projects: {0}")]
    DeleteProject(crate::client::Error),
    #[error("Failed to save workflow integration: {0}")]
    SaveWorkflowIntegration(crate::client::Error),
    #[error("Failed to upload file: {0}")]
    Upload(crate::client::Error),
    #[error("Failed to read the workflow schemas: {0}")]
    Schemas(crate::client::Error),
    #[error("The workflow has no {0}. Save the workflow and wait for the download first")]
    NoSchema(&'static str),
    #[error("Not logged in")]
    NotLoggedIn,
    #[error("{0}")]
    Storage(S::Error),
    #[error("Unable to reqd upload queue")]
    PoisenedUploadQueue,
}

/// Which of the two editable parameter lists of a workflow is meant
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParameterListKind {
    /// Parameters the user sets when starting the workflow
    Dynamic,
    /// Parameters always passed unchanged
    Fixed,
}

pub struct DisassambledWorkflowIntegration {
    inner: WorkflowIntegrationResponse,
    engine_parameters: Vec<Signal<EngineParameter>>,
    dynamic_parameters: Vec<Signal<Parameter>>,
    fixed_parameters: Vec<Signal<Parameter>>,
}

impl DisassambledWorkflowIntegration {
    pub fn inner(&self) -> &WorkflowIntegrationResponse {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut WorkflowIntegrationResponse {
        &mut self.inner
    }

    pub fn engine_parameters(&self) -> &[Signal<EngineParameter>] {
        &self.engine_parameters
    }

    /// Mutable access to the engine parameter rows, see [`Self::engine_parameters`].
    pub fn engine_parameters_mut(&mut self) -> &mut Vec<Signal<EngineParameter>> {
        &mut self.engine_parameters
    }

    pub fn dynamic_parameters(&self) -> &[Signal<Parameter>] {
        &self.dynamic_parameters
    }

    pub fn dynamic_parameters_mut(&mut self) -> &mut Vec<Signal<Parameter>> {
        &mut self.dynamic_parameters
    }

    pub fn fixed_parameters(&self) -> &[Signal<Parameter>] {
        &self.fixed_parameters
    }

    pub fn fixed_parameters_mut(&mut self) -> &mut Vec<Signal<Parameter>> {
        &mut self.fixed_parameters
    }

    /// Applies a reorder or a move between the dynamic and the fixed list. Moving a separator moves
    /// its whole group. A move to the other list is refused if a parameter name is already used
    /// there.
    ///
    /// # Arguments
    /// * `list` - List the request refers to
    /// * `request` - What the user asked for
    pub fn apply_move(
        &mut self,
        list: ParameterListKind,
        request: ParameterMove,
    ) -> Result<(), String> {
        fn is_separator(parameter: &Signal<Parameter>) -> bool {
            matches!(*parameter.read(), Parameter::Group(_))
        }
        let (source, target) = match list {
            ParameterListKind::Dynamic => {
                (&mut self.dynamic_parameters, &mut self.fixed_parameters)
            }
            ParameterListKind::Fixed => (&mut self.fixed_parameters, &mut self.dynamic_parameters),
        };
        match request {
            ParameterMove::Step(index, direction) if index < source.len() => {
                parameter_arrangement::step(source, index, direction, is_separator);
            }
            ParameterMove::Drop {
                from,
                target: drop_target,
            } if from < source.len() && drop_target < source.len() => {
                let range = parameter_arrangement::moving_range(source, from, is_separator);
                if !range.contains(&drop_target) {
                    let destination = parameter_arrangement::drop_destination(
                        source,
                        &range,
                        drop_target,
                        is_separator,
                    );
                    parameter_arrangement::move_range(source, range, destination);
                }
            }
            ParameterMove::ToOtherList(index) if index < source.len() => {
                let range = parameter_arrangement::moving_range(source, index, is_separator);
                let moving: Vec<String> = source[range.clone()]
                    .iter()
                    .map(|parameter| parameter.read().name().to_string())
                    .collect();
                let existing: Vec<String> = target
                    .iter()
                    .map(|parameter| parameter.read().name().to_string())
                    .collect();
                let clashes = parameter_arrangement::name_clashes(
                    moving.iter().map(String::as_str),
                    &existing.iter().map(String::as_str).collect::<Vec<_>>(),
                );
                if !clashes.is_empty() {
                    return Err(format!(
                        "Cannot move: the other list already has a parameter named {}",
                        clashes.join(", ")
                    ));
                }
                target.extend(source.drain(range));
            }
            _ => {}
        }
        Ok(())
    }

    /// Reassembles the disassembled parameter signals back into the inner
    /// response's configuration and returns it, consuming self.
    ///
    pub fn into_response(mut self) -> WorkflowIntegrationResponse {
        *self.inner.configuration_mut().engine_parameters_mut() = self
            .engine_parameters
            .drain(..)
            .map(|param| param.read().clone())
            .collect();

        *self.inner.configuration_mut().dynamic_parameters_mut() = self
            .dynamic_parameters
            .drain(..)
            .map(|param| param.read().clone())
            .collect();

        *self.inner.configuration_mut().fixed_parameters_mut() = self
            .fixed_parameters
            .drain(..)
            .map(|param| param.read().clone())
            .collect();

        self.inner
    }
}

impl From<WorkflowIntegrationResponse> for DisassambledWorkflowIntegration {
    fn from(mut workflow_integration: WorkflowIntegrationResponse) -> Self {
        let engine_parameters = workflow_integration
            .configuration_mut()
            .engine_parameters_mut()
            .drain(..)
            .map(|param| Signal::new(param))
            .collect::<Vec<_>>();

        let dynamic_parameters = workflow_integration
            .configuration_mut()
            .dynamic_parameters_mut()
            .drain(..)
            .map(|param| Signal::new(param))
            .collect::<Vec<_>>();

        let fixed_parameters = workflow_integration
            .configuration_mut()
            .fixed_parameters_mut()
            .drain(..)
            .map(|param| Signal::new(param))
            .collect::<Vec<_>>();

        Self {
            inner: workflow_integration,
            engine_parameters,
            dynamic_parameters,
            fixed_parameters,
        }
    }
}

fn on_workflow_source_change(
    mut workflow_integration: Signal<DisassambledWorkflowIntegration>,
    mut last_source: Signal<Option<Source>>,
    event: Event<FormData>,
) {
    // grab and hold write locks
    let mut workflow_integration = workflow_integration.write();
    let inner_integration = workflow_integration.inner_mut();
    let config = inner_integration.configuration_mut();
    let current_source = config.source_mut();
    let last_source = &mut *last_source.write();

    // Parse new source type
    let new_source_type = match Source::from_str(&event.value()) {
        Ok(source) => source,
        Err(err) => {
            tracing::error!("{err}");
            return;
        }
    };

    // Check if source type actually changed, if not do nothing to avoid resetting source specific fields
    if new_source_type.is_same_variant(current_source) {
        tracing::debug!("Same variants, doing nothing");
        return;
    }

    if last_source
        .as_ref()
        .is_some_and(|last| last.is_same_variant(&new_source_type))
    {
        // Switching back to the previously used variant: restore its settings.
        std::mem::swap(current_source, last_source.as_mut().unwrap());
    } else {
        // Stash the outgoing source so switching back restores it, start fresh with the new variant.
        *last_source = Some(std::mem::replace(current_source, new_source_type));
    }
}

pub trait IsWorkFlowEdit {
    type Storage: IsStorage + 'static;
    type Router: IsRouter + 'static;
}

#[derive(Clone, PartialEq, Props)]
pub struct EditProps {
    pub id: i64,
}

/// Pause between two checks of the repository download state
const PREFETCH_POLL: Duration = Duration::from_secs(1);

/// Checks after which the UI stops following a download (a fetch that long is reported as it is)
const PREFETCH_POLLS: u32 = 300;

/// First characters of a commit hash, for display.
///
/// # Arguments
/// * `commit` - Full commit hash
fn short_commit(commit: &str) -> &str {
    commit.get(..8).unwrap_or(commit)
}

#[component]
pub fn Edit<S: IsStorage + 'static, R: IsRouter + 'static>(props: EditProps) -> Element {
    let storage = use_context::<Signal<S>>();
    let config = use_context::<Signal<Configuration>>();
    let navigator = navigator();

    let mut is_delete_confirmation_dialog_open = use_signal(|| false);
    let mut is_share_dialog_open = use_signal(|| false);
    let sharing = SharingEnabled::from_context().is_enabled();
    let last_workflow_source = use_signal(|| None::<Source>);
    let folder_picker = LocalFolderPicker::from_context();
    // Servers can restrict who creates local workflows; existing local ones stay editable
    let may_create_local =
        CurrentUser::from_context().is_none_or(|user| user.can_create_local_workflows());
    // Zip archive chosen in the file input, waiting to be uploaded
    let mut pending_archive = use_signal(|| None::<Vec<u8>>);
    // Bumped after each pick to remount the file input, which clears its remembered file name
    let mut upload_input_generation = use_signal(|| 0u32);

    // Background download of a remote repository, started by the server when the workflow is
    // saved; shown below the save button
    let mut prefetch = use_signal(|| PrefetchResponse::Idle);
    // Why the last reorder/move between parameter lists was refused
    let mut move_error = use_signal(|| None::<String>);
    // Identifies the newest polling loop, older ones stop when it changes
    let mut prefetch_generation = use_signal(|| 0u32);
    // Follows the download started by a save of a workflow with the given source until it ends.
    // Only remote and nf-core sources are downloaded, for others the state is reset.
    let mut watch_prefetch = move |source: Source| {
        if !(source.is_remote() || source.is_nf_core()) {
            prefetch_generation += 1;
            prefetch.set(PrefetchResponse::Idle);
            return;
        }
        prefetch_generation += 1;
        let generation = *prefetch_generation.peek();
        prefetch.set(PrefetchResponse::Fetching);
        spawn(async move {
            for _ in 0..PREFETCH_POLLS {
                if *prefetch_generation.peek() != generation {
                    return;
                }
                let Some(jwt) = storage.read().jwt().ok().flatten() else {
                    return;
                };
                let config = config.read().clone();
                let state = Client::new(config.base_url().to_string(), config.timeout())
                    .workflow_integrations()
                    .prefetch(&jwt, props.id)
                    .await;
                if *prefetch_generation.peek() != generation {
                    return;
                }
                match state {
                    Ok(PrefetchResponse::Fetching) => {}
                    Ok(state) => {
                        prefetch.set(state);
                        return;
                    }
                    Err(_) => {
                        prefetch.set(PrefetchResponse::Idle);
                        return;
                    }
                }
                sleep(PREFETCH_POLL).await;
            }
        });
    };

    let workflow_integration_res: Resource<
        Result<Signal<DisassambledWorkflowIntegration>, Error<S>>,
    > = use_resource(move || async move {
        let jwt = storage
            .read()
            .jwt()
            .map_err(Error::Storage)?
            .ok_or(Error::NotLoggedIn)?;

        let config = config.read().clone();

        let workflow_integration = Client::new(config.base_url().to_string(), config.timeout())
            .workflow_integrations()
            .workflow_integration(&jwt, props.id)
            .await
            .map_err(Error::FetchProject)?;

        Ok(Signal::new(workflow_integration.into()))
    });

    let mut delete_action = use_action(move || async move {
        let jwt = storage
            .read()
            .jwt()
            .map_err(Error::Storage)?
            .ok_or(Error::NotLoggedIn)?;

        let config = config.read().clone();

        Client::new(config.base_url().to_string(), config.timeout())
            .workflow_integrations()
            .delete(&jwt, props.id)
            .await
            .map_err(Error::DeleteProject)?;

        navigator.push(R::workflow_integration_index_route());

        Ok::<_, Error<S>>(())
    });

    // Appends the parameters of the stored workflow's `nextflow_schema.json` whose names are not
    // used yet. Returns the number of added parameters and the skipped `(name, reason)` pairs.
    let mut import_action = use_action(move || async move {
        let jwt = storage
            .read()
            .jwt()
            .map_err(Error::Storage)?
            .ok_or(Error::NotLoggedIn)?;

        let config = config.read().clone();

        let schemas = Client::new(config.base_url().to_string(), config.timeout())
            .workflow_integrations()
            .schemas(&jwt, props.id)
            .await
            .map_err(Error::Schemas)?;

        let loaded = match &*workflow_integration_res.value().read() {
            Some(Ok(signal)) => Some(*signal),
            _ => None,
        };
        let Some(mut workflow_integration) = loaded else {
            return Err(Error::ConfigNotLoaded);
        };

        let engine = workflow_integration
            .read()
            .inner()
            .configuration()
            .engine()
            .clone();
        let imported = match engine {
            Engine::Nextflow => schemas
                .nextflow_schema
                .as_ref()
                .map(parameters_from_nextflow_schema)
                .ok_or(Error::NoSchema("nextflow_schema.json"))?,
            Engine::Snakemake => {
                if schemas.snakemake_schemas.is_empty() {
                    return Err(Error::NoSchema("Snakemake schema (*.schema.yaml)"));
                }
                // All schema files in path order, the first definition of a name wins
                let mut merged = Imported::default();
                for file in &schemas.snakemake_schemas {
                    merged.merge(parameters_from_snakemake_schema(&file.content), &file.path);
                }
                merged
            }
            Engine::Cwl => {
                let file = schemas
                    .cwl_files
                    .first()
                    .ok_or(Error::NoSchema("CWL main script"))?;
                parameters_from_cwl(&file.content)
            }
            _ => return Err(Error::NoSchema("schema import for this engine")),
        };

        let mut added = 0;
        for parameter in imported.parameters {
            let exists = !parameter.name().is_empty()
                && workflow_integration
                    .read()
                    .dynamic_parameters()
                    .iter()
                    .any(|existing| existing.read().name() == parameter.name());
            if !exists {
                workflow_integration
                    .write()
                    .dynamic_parameters_mut()
                    .push(Signal::new(parameter));
                added += 1;
            }
        }

        Ok::<_, Error<S>>((added, imported.skipped))
    });

    let mut save_action = use_action(move || async move {
        let jwt = storage
            .read()
            .jwt()
            .map_err(Error::Storage)?
            .ok_or(Error::NotLoggedIn)?;

        let config = config.read().clone();

        let mut workflow_integration_signal = match &*workflow_integration_res.read() {
            Some(Ok(signal)) => *signal,
            _ => return Ok::<_, Error<S>>(()),
        };

        let disassembled = {
            let current = workflow_integration_signal.read();
            DisassambledWorkflowIntegration {
                inner: current.inner().clone(),
                engine_parameters: current.engine_parameters().to_vec(),
                dynamic_parameters: current.dynamic_parameters().to_vec(),
                fixed_parameters: current.fixed_parameters().to_vec(),
            }
        };
        let response = disassembled.into_response();

        let updated = Client::new(config.base_url().to_string(), config.timeout())
            .workflow_integrations()
            .update_workflow_integration(
                &jwt,
                props.id,
                response.name.clone(),
                response.description.clone(),
                response.configuration().clone(),
            )
            .await
            .map_err(Error::SaveWorkflowIntegration)?;

        let source = updated.configuration().source().clone();
        workflow_integration_signal.write().inner = updated;
        watch_prefetch(source);

        Ok::<_, Error<S>>(())
    });

    // Saves the workflow (so the stored source is local), then uploads the chosen archive and
    // takes over the source the server set
    let mut upload_action = use_action(move || async move {
        let jwt = storage
            .read()
            .jwt()
            .map_err(Error::Storage)?
            .ok_or(Error::NotLoggedIn)?;
        let config = config.read().clone();
        let Some(archive) = pending_archive.write().take() else {
            return Ok::<_, Error<S>>(());
        };
        let mut workflow_integration_signal = match &*workflow_integration_res.read() {
            Some(Ok(signal)) => *signal,
            _ => return Ok(()),
        };
        let (name, description, configuration, script) = {
            let current = workflow_integration_signal.read();
            let inner = current.inner();
            let script = match inner.configuration().source() {
                Source::Local(local) => Some(local.script().display().to_string()),
                _ => None,
            };
            (
                inner.name.clone(),
                inner.description.clone(),
                inner.configuration().clone(),
                script,
            )
        };
        let client = Client::new(config.base_url().to_string(), config.timeout());
        client
            .workflow_integrations()
            .update_workflow_integration(&jwt, props.id, name, description, configuration)
            .await
            .map_err(Error::SaveWorkflowIntegration)?;
        let updated = client
            .workflow_integrations()
            .upload(&jwt, props.id, archive, script.as_deref())
            .await
            .map_err(Error::Upload)?;
        // Only the source changed: keep edits of parameters that are not saved yet
        let source = updated.configuration().source().clone();
        *workflow_integration_signal
            .write()
            .inner_mut()
            .configuration_mut()
            .source_mut() = source.clone();
        watch_prefetch(source);
        Ok(())
    });

    rsx! {
        match &*workflow_integration_res.read() {
            Some(Ok(mut workflow_integration)) => rsx!{
                ConfirmationDialog {
                    is_open: is_delete_confirmation_dialog_open,
                    title: "Delete workflow integration".to_string(),
                    body: "Are you sure you want to delete this workflow integration? This action cannot be undone.".to_string(),
                    confirmation_button_label: "Delete".to_string(),
                    confirmation_callback: Callback::new(move |_| {
                        is_delete_confirmation_dialog_open.set(false);
                        delete_action.call();
                    }),
                    cancel_callback: Callback::new(move |_| is_delete_confirmation_dialog_open.set(false))
                }
                if sharing {
                    ShareDialog::<S> {
                        kind: ShareKind::Workflow,
                        resource_id: props.id,
                        is_open: is_share_dialog_open,
                        cancel_callback: Callback::new(move |_| is_share_dialog_open.set(false)),
                    }
                }
                div {
                    class: "logo d-flex flex-row justify-content-between mb-3",
                    h1 {
                        class: "mb-0",
                        "{workflow_integration.read().inner().name}"
                        if workflow_integration.read().inner().ownerless {
                            OwnerlessBadge {}
                        }
                    }
                    div {
                        class: "d-flex flex-row",
                        if workflow_integration.read().inner().my_role.is_some_and(|role| role >= Role::Editor) {
                            button {
                                id: "save-btn",
                                class: "btn btn-sm btn-outline-primary me-2",
                                title: "Save workflow integration",
                                onclick: move |_| { save_action.call(); },
                                i {
                                    class: "fa-solid fa-floppy-disk me-1"
                                }
                                "Save"
                            }
                        }
                        if workflow_integration.read().inner().my_role == Some(Role::Owner) {
                            if sharing {
                                button {
                                    id: "share-btn",
                                    class: "btn btn-sm btn-outline-secondary me-2",
                                    title: "Share workflow integration",
                                    onclick: move |_| is_share_dialog_open.set(true),
                                    i {
                                        class: "fa-solid fa-share-nodes me-1"
                                    }
                                    "Share"
                                }
                            }
                            button {
                                id: "delete-btn",
                                class: "btn btn-sm btn-outline-danger",
                                title: "Delete button",
                                onclick: move |_| is_delete_confirmation_dialog_open.set(true),
                                i {
                                    class: "fa-solid fa-trash me-1"
                                }
                                "Delete"
                            }
                        }
                    }
                }
                match delete_action.value() {
                    Some(Err(err)) => rsx! {
                        div {
                            class: "alert alert-danger",
                            role: "alert",
                            "Failed to delete project: {err}" }
                    },
                    _ => rsx! {}
                }
                match save_action.value() {
                    Some(Err(err)) => rsx! {
                        div {
                            class: "alert alert-danger",
                            role: "alert",
                            "Failed to save workflow integration: {err}" }
                    },
                    _ => rsx! {}
                }
                match &*prefetch.read() {
                    PrefetchResponse::Idle => rsx! {},
                    PrefetchResponse::Fetching => rsx! {
                        div {
                            class: "alert alert-info",
                            role: "status",
                            i { class: "fa-solid fa-spinner fa-spin me-2" }
                            "Downloading the workflow repository into the cache..."
                        }
                    },
                    PrefetchResponse::Done { commit } => rsx! {
                        div {
                            class: "alert alert-success",
                            role: "status",
                            "Workflow repository is cached (commit {short_commit(commit)})."
                        }
                    },
                    PrefetchResponse::Failed { message } => rsx! {
                        div {
                            class: "alert alert-warning",
                            role: "alert",
                            "Saved, but the workflow repository could not be downloaded: {message}. \
                             Runs need access to the host until it is cached."
                        }
                    },
                }
                div {
                    class: "input-group mb-3",
                    span {
                        class: "input-group-text",
                        id: "workflow-integration-name",
                        "Name"
                    }
                    input {
                        type: "text",
                        class: "form-control",
                        placeholder: "Workflow integration name",
                        aria_label: "Workflow integration name",
                        aria_describedby: "workflow-integration-name",
                        value: "{workflow_integration.read().inner().name}",
                        oninput: move |event| workflow_integration.write().inner_mut().name = event.value(),
                    }
                }
                div {
                    class: "input-group mb-3",
                    span {
                        class: "input-group-text",
                        id: "workflow-integration-description",
                        "Description"
                    }
                    textarea {
                        class: "form-control",
                        placeholder: "Workflow integration description",
                        aria_label: "Workflow integration description",
                        value: "{workflow_integration.read().inner().description}",
                        oninput: move |event| workflow_integration.write().inner_mut().description = event.value(),
                    }
                }
                h2 {
                    "Configuration"
                },
                p {
                    "This setting defines the kind of workflow (Nextflow, Snakemake, CWL, Container, ...), where to get the workflow source and how the paramter inputs should be rendered for the user."
                }
                Separator{
                    label: "Engine"
                }
                div {
                    class: "input-group mb-3",
                    span {
                        class: "input-group-text",
                        id: "workflow-integration-configuration-engine",
                        "Engine"
                    }
                    select {
                        class: "form-select",
                        aria_label: "Workflow engine",
                        onchange: move |event| {
                            let engine = event.value();
                            if let Ok(engine) = engine.parse::<Engine>() {
                                let mut workflow_integration = workflow_integration.write();
                                let configuration = &mut workflow_integration.inner_mut().configuration;
                                let previous_engine = configuration.engine().clone();
                                // Sources are engine specific: fall back to the engine's first source if the current one is unsupported
                                if !configuration.source().is_supported_by(&engine) {
                                    let first_allowed = engine
                                        .supported_sources()
                                        .into_iter()
                                        .find(|source| may_create_local || !source.is_local());
                                    if let Some(source) = first_allowed {
                                        *configuration.source_mut() = source;
                                    }
                                }
                                // A main script that is still the previous engine's default follows the new engine
                                if let Some(new_default) = engine.default_script() {
                                    let previous_default = configuration.engine().default_script();
                                    match configuration.source_mut() {
                                        Source::Local(local) => {
                                            if previous_default.is_some_and(|d| local.script().as_path() == std::path::Path::new(d)) {
                                                *local.script_mut() = new_default.into();
                                            }
                                        }
                                        Source::Remote(remote) => {
                                            if let (Some(previous), Some(script)) = (previous_default, remote.script_mut()) {
                                                if script.as_path() == std::path::Path::new(previous) {
                                                    *script = new_default.into();
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                                *configuration.engine_mut() = engine;
                                // Versions and engine parameters belong to one engine
                                *configuration.engine_version_mut() = None;
                                if *configuration.engine() != previous_engine {
                                    workflow_integration.engine_parameters_mut().clear();
                                }
                            }
                        },
                        for engine in Engine::iter() {
                            {
                                let selected = *workflow_integration.read().inner().configuration.engine() == engine;
                                let hint = engine_hint(&engine);
                                rsx! {
                                    // The current engine stays selectable so an existing workflow is not changed silently
                                    option {
                                        selected,
                                        disabled: hint.is_some() && !selected,
                                        title: hint,
                                        value: "{engine}",
                                        "{engine}"
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some(hint) = engine_hint(workflow_integration.read().inner().configuration.engine()) {
                    div {
                        class: "alert alert-warning",
                        "{hint}. Runs of this workflow are rejected."
                    }
                }
                ExecutionEnvironmentNote {
                    engine: workflow_integration.read().inner().configuration.engine().clone(),
                }
                // Only Nextflow and CWL leave work files that can be deleted
                if matches!(
                    workflow_integration.read().inner().configuration.engine(),
                    Engine::Nextflow | Engine::Cwl
                ) {
                    div {
                        class: "form-check mb-3",
                        input {
                            class: "form-check-input",
                            r#type: "checkbox",
                            id: "workflow-integration-configuration-cleanup",
                            checked: workflow_integration.read().inner().configuration.cleanup_on_success(),
                            onchange: move |event| {
                                *workflow_integration
                                    .write()
                                    .inner_mut()
                                    .configuration
                                    .cleanup_on_success_mut() = event.checked();
                            },
                        }
                        label {
                            class: "form-check-label",
                            r#for: "workflow-integration-configuration-cleanup",
                            "Delete work files after a successful run"
                        }
                        div {
                            class: "form-text",
                            "Failed and cancelled runs keep their work files, so only those can be resumed."
                        }
                    }
                }
                // Engines without images of their own (`Container`) have no catalog and no versions
                if let Some(catalog) = workflow_integration.read().inner().configuration.engine().catalog() {
                    div {
                        class: "mb-3",
                        div {
                            class: "input-group",
                            span {
                                class: "input-group-text",
                                "Engine version"
                            }
                            select {
                                class: "form-select",
                                id: "workflow-integration-configuration-engine-version",
                                aria_label: "Engine version",
                                onchange: move |event| {
                                    let value = event.value();
                                    *workflow_integration
                                        .write()
                                        .inner_mut()
                                        .configuration
                                        .engine_version_mut() = (!value.is_empty()).then_some(value);
                                },
                                option {
                                    value: "",
                                    selected: workflow_integration.read().inner().configuration.engine_version().is_none(),
                                    "Default ({catalog.default})"
                                }
                                for version in catalog.versions.iter() {
                                    option {
                                        value: "{version}",
                                        selected: workflow_integration.read().inner().configuration.engine_version() == Some(version.as_str()),
                                        "{version}"
                                    }
                                }
                            }
                        }
                        div {
                            class: "form-text",
                            "Version of the engine the workflow runs on. The default may change with updates; pin a version to keep runs reproducible."
                        }
                    }
                }

                EngineParameterEditor { workflow_integration }

                Separator{
                    label: "Source"
                }

                div {
                    class: "input-group mb-3",
                    span {
                        class: "input-group-text",
                        id: "workflow-integration-configuration-source",
                        "Source"
                    }
                    select {
                        class: "form-select",
                        aria_label: "Workflow source",
                        onchange: move |event| on_workflow_source_change(workflow_integration, last_workflow_source, event),
                        for source in workflow_integration.read().inner().configuration.engine().supported_sources() {
                            {
                                // Not offered where the server's policy forbids it, unless the workflow already is local
                                let forbidden = source.is_local()
                                    && !may_create_local
                                    && !workflow_integration.read().inner().configuration.source().is_local();
                                rsx! {
                                    option {
                                        selected: source.is_same_variant(workflow_integration.read().inner().configuration.source()),
                                        value: "{source}",
                                        disabled: forbidden,
                                        title: if forbidden { "Creating local workflows is not allowed for you on this server" } else { "" },
                                        if forbidden {
                                            "{source} - not allowed by administrator"
                                        } else {
                                            "{source}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                match workflow_integration.read().inner().configuration().source() {
                    Source::Local(source) => rsx! {
                        if let Some(picker) = folder_picker.clone() {
                            div {
                                class: "input-group mb-3",
                                span {
                                    class: "input-group-text",
                                    id: "workflow-integration-configuration-source-local-location",
                                    "Location"
                                }
                                input {
                                    type: "text",
                                    class: "form-control",
                                    readonly: true,
                                    placeholder: "No folder chosen",
                                    aria_label: "Folder containing the workflow",
                                    aria_describedby: "workflow-integration-configuration-source-local-location",
                                    value: "{source.location().display()}",
                                }
                                button {
                                    class: "btn btn-outline-primary",
                                    r#type: "button",
                                    onclick: move |_| {
                                        let picker = picker.clone();
                                        spawn(async move {
                                            let start = match workflow_integration.read().inner().configuration().source() {
                                                Source::Local(source) => source.location().clone(),
                                                _ => PathBuf::new(),
                                            };
                                            if let Some(folder) = picker.pick(start).await {
                                                if let Source::Local(source) = workflow_integration.write().inner_mut().configuration_mut().source_mut() {
                                                    *source.location_mut() = folder;
                                                }
                                            }
                                        });
                                    },
                                    i { class: "fa-solid fa-folder-open me-1" }
                                    "Choose folder…"
                                }
                            }
                            div {
                                class: "form-text mb-3",
                                "The files are read from this folder when a run starts and are not copied. Changing them while a run is queued or running can affect that run."
                            }
                        } else {
                            div {
                                class: "mb-3",
                                label {
                                    class: "form-label",
                                    r#for: "workflow-integration-upload",
                                    "Workflow files"
                                }
                                if !may_create_local {
                                    div { class: "form-text", "Uploading workflow files is not allowed for you on this server." }
                                } else {
                                input {
                                    // Browsers fire no change event when the same file name is picked again (e.g. a zip re-created after a fix),
                                    // and a file input's value cannot be set from markup. A new key makes Dioxus replace the element with a fresh,
                                    // empty one after each pick, so no JavaScript is needed to clear it.
                                    key: "{upload_input_generation}",
                                    id: "workflow-integration-upload",
                                    type: "file",
                                    class: "form-control",
                                    accept: ".zip,application/zip",
                                    onchange: move |event| async move {
                                        if let Some(file) = event.files().into_iter().next() {
                                            if let Ok(bytes) = file.read_bytes().await {
                                                pending_archive.set(Some(bytes.to_vec()));
                                                upload_action.call();
                                            }
                                        }
                                        // Remount the input via its key: the browser fires no change event when the same file name is chosen again (re-zipped after a fix)
                                        upload_input_generation += 1;
                                    },
                                }
                                }
                                div {
                                    class: "form-text",
                                    "Upload the workflow as a zip archive. The workflow is saved first and the files are stored on the server. A single top-level folder in the archive is removed."
                                }
                                div {
                                    class: "form-text",
                                    "Every upload is a new version that replaces the previous one for runs started afterwards. A run keeps the version it was started with, so uploading while runs are queued or running does not change them. Old versions are deleted once no run uses them."
                                }
                                if let Some(Err(error)) = upload_action.value().map(|r| r.map(|_| ())) {
                                    div { class: "alert alert-danger mt-2", role: "alert", "{error}" }
                                } else if upload_action.value().is_some() {
                                    div { class: "alert alert-success mt-2", role: "alert", "Uploaded." }
                                }
                            }
                        },
                        div {
                            class: "input-group mb-3",
                            span {
                                class: "input-group-text",
                                id: "workflow-integration-configuration-source-local-script",
                                "Script"
                            }
                            input {
                                type: "text",
                                class: "form-control",
                                placeholder: "Path to main script within location",
                                aria_label: "Path to main script within location",
                                aria_describedby: "workflow-integration-configuration-source-local-script",
                                value: "{source.script().display()}",
                                oninput: move |event| if let Source::Local(source) = workflow_integration.write().inner_mut().configuration_mut().source_mut() {
                                    *source.script_mut() = PathBuf::from(event.value());
                                }
                            }
                        }
                    },
                    Source::Remote(source) => rsx!{
                        div {
                            class: "input-group mb-3",
                            span {
                                class: "input-group-text",
                                id: "workflow-integration-configuration-source-remote-url",
                                "URL"
                            }
                            input {
                                type: "text",
                                class: "form-control",
                                placeholder: "git@github.com:cubimedrub/macworp-genomics-demo-workflow-nextflow.git",
                                aria_label: "URL to the remote repository",
                                aria_describedby: "workflow-integration-configuration-source-remote-url",
                                value: "{source.url()}",
                                oninput: move |event| if let Source::Remote(source) = workflow_integration.write().inner_mut().configuration_mut().source_mut() {
                                    *source.url_mut() = event.value();
                                }
                            }
                        },
                        div {
                            class: "input-group mb-3",
                            span {
                                class: "input-group-text",
                                id: "workflow-integration-configuration-source-remote-version",
                                "Version"
                            }
                            input {
                                type: "text",
                                class: "form-control",
                                placeholder: "main | master | dev | 1.2.1 | #663eee7",
                                aria_label: "Workflow version",
                                aria_describedby: "workflow-integration-configuration-source-remote-version",
                                value: "{source.version().map(|version| version.as_str()).unwrap_or_default()}",
                                oninput: move |event| if let Source::Remote(source) = workflow_integration.write().inner_mut().configuration_mut().source_mut() {
                                    *source.version_mut() = if event.value().is_empty() { None } else { Some(event.value()) }
                                }
                            }
                        }
                        div {
                            class: "input-group mb-3",
                            span {
                                class: "input-group-text",
                                id: "workflow-integration-configuration-source-remote-script",
                                "Script"
                            }
                            input {
                                type: "text",
                                class: "form-control",
                                placeholder: "Path to main script within location",
                                aria_label: "Path to main script within location",
                                aria_describedby: "workflow-integration-configuration-source-remote-script",
                                value: "{source.script().map(|path| path.display().to_string()).unwrap_or_default()}",
                                oninput: move |event| if let Source::Remote(source) = workflow_integration.write().inner_mut().configuration_mut().source_mut() {
                                    *source.script_mut() = if event.value().is_empty() { None } else { Some(PathBuf::from(event.value())) }
                                }
                            }
                        }
                    },
                    Source::Image(source) => rsx! {
                        div {
                            class: "input-group mb-3",
                            span {
                                class: "input-group-text",
                                id: "workflow-integration-configuration-source-image",
                                "Image"
                            }
                            input {
                                type: "text",
                                class: "form-control",
                                placeholder: "docker.io/library/hello-world:latest",
                                aria_label: "Container image to pull",
                                aria_describedby: "workflow-integration-configuration-source-image",
                                value: "{source.image()}",
                                oninput: move |event| if let Source::Image(source) = workflow_integration.write().inner_mut().configuration_mut().source_mut() {
                                    *source.image_mut() = event.value();
                                }
                            }
                        }
                    },
                    Source::NfCore(source) => rsx! {
                        NfCoreSourceFields::<S> {
                            pipeline: source.pipeline().to_string(),
                            version: source.version().cloned(),
                            on_pipeline: move |pipeline| if let Source::NfCore(source) = workflow_integration.write().inner_mut().configuration_mut().source_mut() {
                                *source.pipeline_mut() = pipeline;
                            },
                            on_version: move |version| if let Source::NfCore(source) = workflow_integration.write().inner_mut().configuration_mut().source_mut() {
                                *source.version_mut() = version;
                            }
                        }
                    }
                }

                Separator{
                    label: "Dynamic parameters"
                }
                p {
                    "These parameters can be changed by the user when selecting the workflow for execution"
                }

                if matches!(workflow_integration.read().inner().configuration().engine(), Engine::Snakemake) {
                    div {
                        class: "form-text mb-3",
                        "Snakemake: a parameter name is the path of a key in the workflow configuration. A dot separates levels, so "
                        code { "ref.species" }
                        " is passed as "
                        code { "--config \"ref={{'species': ...}}\"" }
                        ". Write "
                        code { "\\." }
                        " for a dot that is part of a key (e.g. "
                        code { "ref.v1\\.2" }
                        ") and "
                        code { "\\\\" }
                        " for a backslash. A name must not be both a value and a parent of other names."
                    }
                }

                if matches!(workflow_integration.read().inner().configuration().engine(), Engine::Nextflow | Engine::Snakemake | Engine::Cwl)
                    && workflow_integration.read().inner().my_role.is_some_and(|role| role >= Role::Editor)
                {
                    div {
                        class: "mb-4",
                        button {
                            id: "import-schema-btn",
                            r#type: "button",
                            class: "btn btn-sm btn-outline-secondary",
                            title: "Add the parameters of the saved workflow's schema (nextflow_schema.json, Snakemake *.schema.yaml or the inputs of the CWL main script)",
                            onclick: move |_| { import_action.call(); },
                            i { class: "fa-solid fa-file-import me-1" }
                            "Import from schema"
                        }
                        div {
                            class: "form-text",
                            "Save the workflow first and wait until the download has finished: the schema is read from the saved workflow. Existing parameters with the same name are kept. Changing the version or branch of the workflow does not update the parameters automatically: they are not merged with a changed schema, so import again and review them."
                        }
                        match import_action.value() {
                            Some(Err(error)) => rsx! {
                                div { class: "alert alert-danger mt-2", role: "alert", "{error}" }
                            },
                            Some(Ok(result)) => {
                                let (added, skipped) = &*result.read();
                                rsx! {
                                    div {
                                        class: "alert alert-success mt-2",
                                        role: "alert",
                                        "Added {added} parameter(s)."
                                        if !skipped.is_empty() {
                                            ul {
                                                class: "mb-0",
                                                for (name, reason) in skipped.iter() {
                                                    li { "{name}: {reason}" }
                                                }
                                            }
                                        }
                                    }
                                }
                            },
                            None => rsx! {},
                        }
                    }
                }

                if let Some(message) = move_error.read().as_ref() {
                    div { class: "alert alert-warning", role: "alert", "{message}" }
                }
                ParameterList<DynamicParameter, S> {
                    on_move: move |request| {
                        let result = workflow_integration.write().apply_move(ParameterListKind::Dynamic, request);
                        move_error.set(result.err());
                    },
                    parameters: workflow_integration.read().dynamic_parameters().to_vec(),
                    on_remove: move |idx| {
                        workflow_integration.write().dynamic_parameters_mut().remove(idx);
                    },
                    on_add: move |parameter| {
                        workflow_integration.write().dynamic_parameters_mut().push(Signal::new(parameter));
                    },
                }

                Separator{
                    label: "Fixed parameters"
                }
                p {
                    "These parameters are always passed to the workflow and cannot be changed by the user"
                }

                if matches!(workflow_integration.read().inner().configuration().engine(), Engine::Snakemake) {
                    div {
                        class: "form-text mb-3",
                        "Snakemake: a parameter name is the path of a key in the workflow configuration. A dot separates levels, so "
                        code { "ref.species" }
                        " is passed as "
                        code { "--config \"ref={{'species': ...}}\"" }
                        ". Write "
                        code { "\\." }
                        " for a dot that is part of a key (e.g. "
                        code { "ref.v1\\.2" }
                        ") and "
                        code { "\\\\" }
                        " for a backslash. A name must not be both a value and a parent of other names."
                    }
                }
                ParameterList<FixedParameter, S> {
                    on_move: move |request| {
                        let result = workflow_integration.write().apply_move(ParameterListKind::Fixed, request);
                        move_error.set(result.err());
                    },
                    parameters: workflow_integration.read().fixed_parameters().to_vec(),
                    on_remove: move |idx| {
                        workflow_integration.write().fixed_parameters_mut().remove(idx);
                    },
                    on_add: move |parameter| {
                        workflow_integration.write().fixed_parameters_mut().push(Signal::new(parameter));
                    },
                }
            },
            Some(Err(err)) => rsx!(
                div {
                    class: "alert alert-danger",
                    "{err}"
                }
            ),
            None => rsx!(
                Spinner {}
            )
        }
    }
}

/// Editor of the engine parameters of a workflow: one row per parameter with the allowed names of
/// the engine to pick from, a value field (hidden for switches) and a remove button. Names and
/// values are checked by the server when saving (`Engine::check_parameter`); the row only hints
/// at what the engine accepts. Nothing is shown for engines without parameters (`Container`).
///
/// # Arguments
/// * `workflow_integration` - The workflow being edited
#[component]
fn EngineParameterEditor(workflow_integration: Signal<DisassambledWorkflowIntegration>) -> Element {
    let engine = workflow_integration
        .read()
        .inner()
        .configuration
        .engine()
        .clone();
    let specs = engine.parameter_specs();
    if specs.is_empty() {
        return rsx! {};
    }
    let rows = workflow_integration.read().engine_parameters().to_vec();
    rsx! {
        div {
            class: "mb-3",
            div {
                class: "d-flex justify-content-between align-items-center mb-1",
                label {
                    class: "form-label mb-0",
                    "Engine parameters"
                }
                button {
                    class: "btn btn-sm btn-outline-secondary",
                    r#type: "button",
                    onclick: move |_| {
                        let first = engine.parameter_specs()[0].name;
                        workflow_integration
                            .write()
                            .engine_parameters_mut()
                            .push(Signal::new(EngineParameter::new(first.into(), String::new())));
                    },
                    "Add parameter"
                }
            }
            for (index, mut row) in rows.into_iter().enumerate() {
                {
                    let current = row.read().clone();
                    let spec = specs.iter().find(|spec| spec.name == current.name());
                    let is_flag = spec.is_some_and(|spec| spec.is_flag());
                    let description = spec.map(|spec| spec.description).unwrap_or("Not allowed for this engine");
                    rsx! {
                        div {
                            key: "{index}",
                            class: "input-group mb-1",
                            select {
                                class: "form-select",
                                style: "max-width: 16rem",
                                aria_label: "Engine parameter",
                                title: description,
                                onchange: move |event| {
                                    // The value of another parameter rarely fits, start empty
                                    row.set(EngineParameter::new(event.value(), String::new()));
                                },
                                for spec in specs.iter() {
                                    option {
                                        value: spec.name,
                                        selected: spec.name == current.name(),
                                        title: spec.description,
                                        "{spec.name}"
                                    }
                                }
                                // A stored name the allowlist does not know stays visible so it can be removed
                                if spec.is_none() {
                                    option {
                                        value: "{current.name()}",
                                        selected: true,
                                        "{current.name()} (not allowed)"
                                    }
                                }
                            }
                            if !is_flag {
                                input {
                                    class: "form-control",
                                    aria_label: "Engine parameter value",
                                    value: "{current.value()}",
                                    oninput: move |event| {
                                        let name = row.read().name().to_string();
                                        row.set(EngineParameter::new(name, event.value()));
                                    },
                                }
                            } else {
                                span {
                                    class: "input-group-text text-muted",
                                    "{description}"
                                }
                            }
                            button {
                                class: "btn btn-outline-danger",
                                r#type: "button",
                                aria_label: "Remove engine parameter",
                                onclick: move |_| {
                                    workflow_integration.write().engine_parameters_mut().remove(index);
                                },
                                "Remove"
                            }
                        }
                    }
                }
            }
            div {
                class: "form-text",
                "Options passed to the engine. A retried run adds the engine's resume option by itself."
            }
        }
    }
}
