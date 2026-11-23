use std::path::PathBuf;

use dioxus::prelude::*;
use macworp_core::{
    sample_sheet::{CellKind, Column, ColumnKind, Format, ListingChecker, SampleFiles, Sheet},
    schema_validation::{AssumeExisting, SchemaValidator, ValidationIssue},
    workflow_configuration::parameters::{parameter::IsParameterWithValue, Parameter},
};
use serde_json::Value;

use crate::{
    client::Client,
    components::{
        projects::file_picker::ProjectFilePicker,
        workflow_integrations::parameter::IsParameter as IsParameterElement,
    },
    configuration::Configuration,
    storage::IsStorage,
};

/// Props of [`SampleSheetParameter`].
#[derive(Clone, PartialEq, Props)]
pub struct SampleSheetProps {
    /// The sample sheet parameter; its value is the project relative path of the sheet file
    pub parameter: Signal<Parameter>,
    /// Position in the parameter list, for element ids
    pub parameter_idx: usize,
    /// Project holding the sheet and the files it references
    pub project_id: i64,
    /// Workflow whose sample sheet schema describes the columns
    pub workflow_id: i64,
}

/// What the inline file picker currently serves.
#[derive(Clone, Copy, PartialEq)]
enum Picking {
    /// Choosing the file of one cell (row, column)
    Cell(usize, usize),
    /// Choosing a folder whose files become rows
    Folder,
    /// Choosing the sheet file itself
    SheetFile,
}

/// Outcome message under the editor: `(is_error, text)`.
type Status = Option<(bool, String)>;

/// Sample sheet parameter in the start dialog: the sheet is a file of the project (CSV, TSV,
/// JSON or YAML, chosen by the extension). The editor loads it, shows it as a grid whose columns
/// follow the workflow's sample sheet schema, marks problems per cell, adds rows from the files of a folder and saves it back. The parameter value stays the path of the file.
#[component]
pub fn SampleSheetParameter<P: IsParameterElement + 'static, S: IsStorage + 'static>(
    mut props: SampleSheetProps,
) -> Element {
    let storage = use_context::<Signal<S>>();
    let configuration = use_context::<Signal<Configuration>>();
    let project_id = props.project_id;
    let workflow_id = props.workflow_id;

    let mut sheet: Signal<Option<Sheet>> = use_signal(|| None);
    let mut dirty = use_signal(|| false);
    let mut status: Signal<Status> = use_signal(|| None);
    let mut picking: Signal<Option<Picking>> = use_signal(|| None);
    // Whether adding rows from a folder pairs read 1/read 2 files
    let mut pair_reads = use_signal(|| false);
    // Existence problems from the last explicit file check; any edit clears them
    let mut file_issues: Signal<Vec<ValidationIssue>> = use_signal(Vec::new);

    let schema = use_resource(move || async move {
        let jwt = storage.read().jwt().ok().flatten()?;
        let configuration = configuration.read().clone();
        Client::new(
            configuration.base_url().to_string(),
            configuration.timeout(),
        )
        .workflow_integrations()
        .schemas(&jwt, workflow_id)
        .await
        .ok()
        .and_then(|schemas| schemas.input_schema)
    });
    let input_schema: Option<Value> = schema.read().clone().flatten();
    let columns: Vec<Column> = input_schema
        .as_ref()
        .map(Column::from_schema)
        .unwrap_or_default();

    // The schema may arrive after a sheet was created or loaded (it is fetched in the
    // background): add its columns as soon as it is there
    use_effect(move || {
        let schema = schema.read().clone().flatten();
        let Some(schema) = schema else {
            return;
        };
        let columns = Column::from_schema(&schema);
        let missing = sheet.read().as_ref().is_some_and(|current| {
            columns
                .iter()
                .any(|column| !current.columns.contains(&column.name))
        });
        if missing {
            if let Some(current) = sheet.write().as_mut() {
                current.ensure_columns(&columns);
            }
        }
    });

    // Problems that need no file listing, recomputed on every edit
    let live_issues = use_memo(move || {
        let schema = schema.read().clone().flatten();
        let (Some(schema), Some(sheet)) = (schema, sheet.read().clone()) else {
            return Vec::new();
        };
        let Ok(validator) = SchemaValidator::new(&schema) else {
            return Vec::new();
        };
        let columns = Column::from_schema(&schema);
        sheet.validate(&validator, &columns, &AssumeExisting)
    });
    let issues: Vec<ValidationIssue> = live_issues
        .read()
        .iter()
        .chain(file_issues.read().iter())
        .cloned()
        .collect();

    let path_text: String = match &*props.parameter.read() {
        Parameter::SampleSheet(parameter) => parameter.value().cloned().unwrap_or_default(),
        _ => {
            return rsx! { div { class: "alert alert-danger", "Unsupported parameter type" } };
        }
    };

    // Applies `change` to the sheet and marks it as modified
    let mut modify = move |change: Box<dyn FnOnce(&mut Sheet)>| {
        if let Some(current) = sheet.write().as_mut() {
            change(current);
        }
        dirty.set(true);
        file_issues.set(Vec::new());
    };

    let load_columns = columns.clone();
    let load = move |_| {
        let path = match &*props.parameter.read() {
            Parameter::SampleSheet(parameter) => parameter.value().cloned().unwrap_or_default(),
            _ => String::new(),
        };
        let columns = load_columns.clone();
        spawn(async move {
            let result = async {
                let format = Format::from_path(std::path::Path::new(&path))
                    .map_err(|error| error.to_string())?;
                let jwt = storage
                    .read()
                    .jwt()
                    .ok()
                    .flatten()
                    .ok_or("Not logged in".to_string())?;
                let configuration = configuration.read().clone();
                let text = Client::new(
                    configuration.base_url().to_string(),
                    configuration.timeout(),
                )
                .projects()
                .read_text(&jwt, project_id, PathBuf::from(&path))
                .await
                .map_err(|error| error.to_string())?
                .ok_or_else(|| {
                    format!("No sample sheet found at {path}. Use \"New\" to create one.")
                })?;
                let mut loaded = Sheet::parse(format, &text).map_err(|error| error.to_string())?;
                loaded.ensure_columns(&columns);
                Ok::<_, String>(loaded)
            }
            .await;
            match result {
                Ok(loaded) => {
                    sheet.set(Some(loaded));
                    dirty.set(false);
                    file_issues.set(Vec::new());
                    status.set(Some((false, format!("Loaded {path}"))));
                }
                Err(message) => status.set(Some((true, message))),
            }
        });
    };

    let save_columns = columns.clone();
    let save = move |_| {
        let path = match &*props.parameter.read() {
            Parameter::SampleSheet(parameter) => parameter.value().cloned().unwrap_or_default(),
            _ => String::new(),
        };
        let Some(current) = sheet.read().clone() else {
            return;
        };
        let kinds = current.kinds(&save_columns);
        spawn(async move {
            let result = async {
                let format = Format::from_path(std::path::Path::new(&path))
                    .map_err(|error| error.to_string())?;
                let jwt = storage
                    .read()
                    .jwt()
                    .ok()
                    .flatten()
                    .ok_or("Not logged in".to_string())?;
                let configuration = configuration.read().clone();
                Client::new(
                    configuration.base_url().to_string(),
                    configuration.timeout(),
                )
                .projects()
                .write_text(
                    &jwt,
                    project_id,
                    PathBuf::from(&path),
                    current.serialize(format, &kinds),
                )
                .await
                .map_err(|error| error.to_string())
            }
            .await;
            match result {
                Ok(()) => {
                    dirty.set(false);
                    status.set(Some((false, format!("Saved {path}"))));
                }
                Err(message) => status.set(Some((true, message))),
            }
        });
    };

    let check_schema = input_schema.clone();
    let check_columns = columns.clone();
    let check_files = move |_| {
        let (Some(schema), Some(current)) = (check_schema.clone(), sheet.read().clone()) else {
            return;
        };
        let columns = check_columns.clone();
        spawn(async move {
            let jwt = storage.read().jwt().ok().flatten();
            let configuration = configuration.read().clone();
            let Some(jwt) = jwt else {
                return;
            };
            let client = Client::new(
                configuration.base_url().to_string(),
                configuration.timeout(),
            );
            let mut checker = ListingChecker::default();
            for directory in current.referenced_directories(&columns) {
                if let Ok(listing) = client
                    .projects()
                    .project(&jwt, project_id, PathBuf::from(&directory))
                    .await
                {
                    checker.add_listing(&directory, &listing.files, &listing.directories);
                }
            }
            match SchemaValidator::new(&schema) {
                Ok(validator) => {
                    let missing: Vec<ValidationIssue> = current
                        .validate(&validator, &columns, &checker)
                        .into_iter()
                        .filter(|issue| issue.keyword == "exists")
                        .collect();
                    status.set(Some((
                        !missing.is_empty(),
                        if missing.is_empty() {
                            "All referenced files exist".to_string()
                        } else {
                            format!("{} referenced path(s) not found", missing.len())
                        },
                    )));
                    file_issues.set(missing);
                }
                Err(error) => status.set(Some((true, error.to_string()))),
            }
        });
    };

    let folder_columns = columns.clone();
    let add_folder = move |folder: PathBuf| {
        picking.set(None);
        let columns = folder_columns.clone();
        let pair_reads = *pair_reads.read();
        spawn(async move {
            let Some(jwt) = storage.read().jwt().ok().flatten() else {
                return;
            };
            let configuration = configuration.read().clone();
            let listing = Client::new(
                configuration.base_url().to_string(),
                configuration.timeout(),
            )
            .projects()
            .project(&jwt, project_id, folder.clone())
            .await;
            match listing {
                Ok(listing) => {
                    // The file column's kind decides what a row may point to: folders count for
                    // `path` columns (a `.d` run is a folder), not for `file-path` columns
                    let file_column = columns
                        .iter()
                        .find(|column| matches!(column.kind, ColumnKind::File | ColumnKind::Path));
                    let with_folders =
                        file_column.is_some_and(|column| column.kind == ColumnKind::Path);
                    let mut entries: Vec<&String> = listing.files.iter().collect();
                    if with_folders {
                        entries.extend(listing.directories.iter());
                    }
                    let files: Vec<String> = entries
                        .into_iter()
                        .map(|name| folder.join(name).to_string_lossy().into_owned())
                        .collect();
                    // Only entries the file column accepts
                    let pattern = file_column
                        .and_then(|column| column.pattern.as_deref())
                        .and_then(|pattern| regex::Regex::new(pattern).ok());
                    let samples = SampleFiles::group(&files, pattern.as_ref(), pair_reads);
                    if samples.is_empty() {
                        status.set(Some((true, "No matching files in that folder".to_string())));
                        return;
                    }
                    let count = samples.len();
                    if sheet.read().is_none() {
                        sheet.set(Some(Sheet::with_columns(&columns)));
                    }
                    if let Some(current) = sheet.write().as_mut() {
                        current.append_samples(&samples, &columns);
                    }
                    dirty.set(true);
                    file_issues.set(Vec::new());
                    status.set(Some((false, format!("Added {count} sample(s)"))));
                }
                Err(error) => status.set(Some((true, error.to_string()))),
            }
        });
    };

    let new_columns = columns.clone();
    let current_sheet = sheet.read().clone();

    rsx! {
        div {
            class: "input-group mb-2",
            span {
                class: "input-group-text",
                id: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-value",
                "{props.parameter.read().label()}"
            }
            input {
                r#type: "text",
                class: "form-control",
                placeholder: "Sample sheet file in the project, e.g. samplesheet.csv",
                aria_label: "Sample sheet path",
                aria_describedby: "workflow-integration-configuration-{P::ID_PART}-parameter-{props.parameter_idx}-value",
                value: "{path_text}",
                oninput: move |event| {
                    if let Parameter::SampleSheet(parameter) = &mut *props.parameter.write() {
                        *parameter.value_mut() = if event.value().is_empty() { None } else { Some(event.value()) };
                    }
                },
            }
            button {
                class: "btn btn-outline-secondary",
                r#type: "button",
                title: "Select the sample sheet from the project files",
                onclick: move |_| {
                    let next = if *picking.read() == Some(Picking::SheetFile) { None } else { Some(Picking::SheetFile) };
                    picking.set(next);
                },
                i { class: "fas fa-folder-open" }
            }
            button {
                class: "btn btn-outline-secondary",
                r#type: "button",
                disabled: path_text.is_empty(),
                title: "Load the file from the project",
                onclick: load,
                i { class: "fas fa-file-import me-1" }
                "Load"
            }
            button {
                class: "btn btn-outline-secondary",
                r#type: "button",
                title: "Start an empty sheet with the columns of the schema",
                onclick: move |_| {
                    sheet.set(Some(Sheet::with_columns(&new_columns)));
                    dirty.set(true);
                    file_issues.set(Vec::new());
                    if path_text.is_empty() {
                        if let Parameter::SampleSheet(parameter) = &mut *props.parameter.write() {
                            *parameter.value_mut() = Some("samplesheet.csv".to_string());
                        }
                    }
                },
                i { class: "fas fa-plus me-1" }
                "New"
            }
        }
        small {
            class: "form-text text-muted d-block mb-2",
            "{props.parameter.read().desc()}"
        }
        if *picking.read() == Some(Picking::SheetFile) {
            ProjectFilePicker::<S> {
                project_id,
                selected: if path_text.is_empty() { Vec::new() } else { vec![PathBuf::from(&path_text)] },
                accepts_files: true,
                accepts_folders: false,
                file_allowed: Callback::new(|path: PathBuf| Format::from_path(&path).is_ok()),
                on_toggle: move |path: PathBuf| {
                    if let Parameter::SampleSheet(parameter) = &mut *props.parameter.write() {
                        *parameter.value_mut() = Some(path.to_string_lossy().into_owned());
                    }
                    picking.set(None);
                },
            }
        }
        if let Some(current) = current_sheet {
            div {
                class: "d-flex flex-wrap gap-2 mb-2",
                button {
                    class: "btn btn-sm btn-outline-secondary",
                    r#type: "button",
                    onclick: {
                        let columns = columns.clone();
                        move |_| modify(Box::new({
                            let columns = columns.clone();
                            move |sheet: &mut Sheet| sheet.add_row(&columns)
                        }))
                    },
                    i { class: "fas fa-plus me-1" }
                    "Add row"
                }
                button {
                    class: "btn btn-sm btn-outline-secondary",
                    r#type: "button",
                    title: "Add one row per file of a folder (only files the sheet's file column accepts)",
                    onclick: move |_| {
                        let next = if *picking.read() == Some(Picking::Folder) { None } else { Some(Picking::Folder) };
                        picking.set(next);
                    },
                    i { class: "fas fa-folder-plus me-1" }
                    "Add rows from folder…"
                }
                div {
                    class: "form-check align-self-center mb-0",
                    input {
                        r#type: "checkbox",
                        class: "form-check-input",
                        id: "sample-sheet-pair-reads-{props.parameter_idx}",
                        checked: *pair_reads.read(),
                        onchange: move |event| pair_reads.set(event.checked()),
                    }
                    label {
                        class: "form-check-label",
                        r#for: "sample-sheet-pair-reads-{props.parameter_idx}",
                        title: "Files that only differ in _1/_2 or _R1/_R2 become one row with two files",
                        "Pair read 1/2 files"
                    }
                }
                button {
                    class: "btn btn-sm btn-outline-secondary",
                    r#type: "button",
                    disabled: input_schema.is_none(),
                    title: "Check that the referenced files exist in the project",
                    onclick: check_files,
                    i { class: "fas fa-circle-check me-1" }
                    "Check files"
                }
                button {
                    class: if *dirty.read() { "btn btn-sm btn-primary ms-auto" } else { "btn btn-sm btn-outline-secondary ms-auto" },
                    r#type: "button",
                    disabled: path_text.is_empty(),
                    title: "Write the sheet to the project file",
                    onclick: save,
                    i { class: "fas fa-floppy-disk me-1" }
                    if *dirty.read() { "Save (unsaved changes)" } else { "Save" }
                }
            }
            if *picking.read() == Some(Picking::Folder) {
                ProjectFilePicker::<S> {
                    project_id,
                    selected: Vec::new(),
                    accepts_files: false,
                    accepts_folders: true,
                    accepts_current: true,
                    on_toggle: add_folder,
                }
            }
            if let Some(Picking::Cell(row, column)) = *picking.read() {
                {
                    let name = current.columns.get(column).cloned().unwrap_or_default();
                    let kind = columns.iter().find(|c| c.name == name).map(|c| c.kind.clone());
                    let selected = current
                        .rows
                        .get(row)
                        .and_then(|cells| cells.get(column))
                        .filter(|value| !value.is_empty())
                        .map(|value| vec![PathBuf::from(value)])
                        .unwrap_or_default();
                    rsx! {
                        ProjectFilePicker::<S> {
                            project_id,
                            selected,
                            accepts_files: !matches!(kind, Some(ColumnKind::Directory)),
                            accepts_folders: matches!(kind, Some(ColumnKind::Directory | ColumnKind::Path)),
                            on_toggle: move |path: PathBuf| {
                                let value = path.to_string_lossy().into_owned();
                                modify(Box::new(move |sheet: &mut Sheet| sheet.set_cell(row, column, value)));
                                picking.set(None);
                            },
                        }
                    }
                }
            }
            if !issues.is_empty() {
                div {
                    class: "alert alert-warning py-2 mb-2",
                    strong { "{issues.len()} problem(s)" }
                    ul {
                        class: "mb-0",
                        for issue in issues.iter().take(8) {
                            li {
                                match issue.row {
                                    Some(row) => format!("Row {}{}: {}", row + 1, issue.column.as_ref().map(|c| format!(", {c}")).unwrap_or_default(), issue.message),
                                    None => issue.message.clone(),
                                }
                            }
                        }
                        if issues.len() > 8 {
                            li { "… and {issues.len() - 8} more" }
                        }
                    }
                }
            }
            div {
                class: "table-responsive mb-2",
                table {
                    class: "table table-sm align-middle",
                    thead {
                        tr {
                            th { "#" }
                            for name in current.columns.iter() {
                                {
                                    let column = columns.iter().find(|c| &c.name == name);
                                    rsx! {
                                        th {
                                            key: "{name}",
                                            title: column.and_then(|c| c.description.clone()).unwrap_or_default(),
                                            "{name}"
                                            if column.is_some_and(|c| c.required) { span { class: "text-danger", " *" } }
                                        }
                                    }
                                }
                            }
                            th {}
                        }
                    }
                    tbody {
                        for (row_index, row) in current.rows.iter().enumerate() {
                            tr {
                                key: "{row_index}",
                                td { class: "text-muted", "{row_index + 1}" }
                                for (column_index, name) in current.columns.iter().enumerate() {
                                    {
                                        let column = columns.iter().find(|c| &c.name == name).cloned();
                                        let value = row.get(column_index).cloned().unwrap_or_default();
                                        let message = issues
                                            .iter()
                                            .find(|i| i.row == Some(row_index) && i.column.as_deref() == Some(name.as_str()))
                                            .map(|i| i.message.clone());
                                        let invalid = if message.is_some() { " is-invalid" } else { "" };
                                        let title = message.clone().unwrap_or_default();
                                        let kind = column.as_ref().map(|c| c.kind.clone());
                                        rsx! {
                                            td {
                                                key: "{name}",
                                                match kind {
                                                    Some(ColumnKind::Choice(options)) => rsx! {
                                                        select {
                                                            class: "form-select form-select-sm{invalid}",
                                                            title: "{title}",
                                                            onchange: move |event| {
                                                                let value = event.value();
                                                                modify(Box::new(move |sheet: &mut Sheet| sheet.set_cell(row_index, column_index, value)));
                                                            },
                                                            option { value: "", selected: value.is_empty(), "" }
                                                            for option in options {
                                                                option { value: "{option}", selected: option == value, "{option}" }
                                                            }
                                                        }
                                                    },
                                                    Some(ColumnKind::Value(CellKind::Boolean)) => rsx! {
                                                        select {
                                                            class: "form-select form-select-sm{invalid}",
                                                            title: "{title}",
                                                            onchange: move |event| {
                                                                let value = event.value();
                                                                modify(Box::new(move |sheet: &mut Sheet| sheet.set_cell(row_index, column_index, value)));
                                                            },
                                                            option { value: "", selected: value.is_empty(), "" }
                                                            option { value: "true", selected: value == "true", "true" }
                                                            option { value: "false", selected: value == "false", "false" }
                                                        }
                                                    },
                                                    Some(ColumnKind::File | ColumnKind::Directory | ColumnKind::Path) => rsx! {
                                                        div {
                                                            class: "input-group input-group-sm",
                                                            input {
                                                                r#type: "text",
                                                                class: "form-control{invalid}",
                                                                title: "{title}",
                                                                value: "{value}",
                                                                oninput: move |event| {
                                                                    let value = event.value();
                                                                    modify(Box::new(move |sheet: &mut Sheet| sheet.set_cell(row_index, column_index, value)));
                                                                },
                                                            }
                                                            button {
                                                                class: "btn btn-outline-secondary",
                                                                r#type: "button",
                                                                title: "Pick from project files",
                                                                onclick: move |_| {
                                                                    let target = Picking::Cell(row_index, column_index);
                                                                    let next = if *picking.read() == Some(target) { None } else { Some(target) };
                                                                    picking.set(next);
                                                                },
                                                                i { class: "fas fa-folder-open" }
                                                            }
                                                        }
                                                    },
                                                    other => rsx! {
                                                        input {
                                                            r#type: if matches!(other, Some(ColumnKind::Value(CellKind::Integer | CellKind::Number))) { "number" } else { "text" },
                                                            class: "form-control form-control-sm{invalid}",
                                                            title: "{title}",
                                                            value: "{value}",
                                                            oninput: move |event| {
                                                                let value = event.value();
                                                                modify(Box::new(move |sheet: &mut Sheet| sheet.set_cell(row_index, column_index, value)));
                                                            },
                                                        }
                                                    },
                                                }
                                            }
                                        }
                                    }
                                }
                                td {
                                    class: "text-nowrap",
                                    button {
                                        class: "btn btn-sm btn-link",
                                        r#type: "button",
                                        title: "Duplicate row",
                                        onclick: move |_| modify(Box::new(move |sheet: &mut Sheet| sheet.duplicate_row(row_index))),
                                        i { class: "fas fa-clone" }
                                    }
                                    button {
                                        class: "btn btn-sm btn-link text-danger",
                                        r#type: "button",
                                        title: "Remove row",
                                        onclick: move |_| {
                                            picking.set(None);
                                            modify(Box::new(move |sheet: &mut Sheet| sheet.remove_row(row_index)));
                                        },
                                        i { class: "fas fa-trash" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if let Some((is_error, message)) = status.read().clone() {
            div {
                class: if is_error { "alert alert-danger py-2" } else { "alert alert-success py-2" },
                "{message}"
            }
        }
        if schema.read().is_some() && input_schema.is_none() {
            small { class: "text-muted d-block mb-2", "This workflow has no sample sheet schema: columns come from the file." }
        }
    }
}
