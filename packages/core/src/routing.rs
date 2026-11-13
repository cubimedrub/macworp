pub trait IsRouting {
    const CONTROLLER_PATH: &str;

    fn controller_path() -> &'static str {
        Self::CONTROLLER_PATH
    }
}

pub struct UtilitiesRouting;
impl UtilitiesRouting {
    pub const HEALTHCHECK_PATH: &str = "/healthcheck";
    pub const VERSION_PATH: &str = "/version";
    /// GET: engines no worker can run
    pub const ENGINES_PATH: &str = "/engines";
    /// GET: pipelines of the public nf-core index
    pub const NF_CORE_PIPELINES_PATH: &str = "/nf-core/pipelines";
    /// GET: releases of one nf-core pipeline
    pub const NF_CORE_RELEASES_PATH: &str = "/nf-core/pipelines/{name}/releases";
}

impl IsRouting for UtilitiesRouting {
    const CONTROLLER_PATH: &str = "/utilities";
}

pub struct UsersRouting;
impl UsersRouting {
    /// GET: the authenticated user
    pub const CURRENT_PATH: &str = "/current";
    /// GET: users whose username starts with the query parameter `q`, to pick share principals
    pub const SEARCH_PATH: &str = "/search";
    /// GET (admin): all users, optionally filtered by the query parameter `q`
    pub const INDEX_PATH: &str = "/index";
    /// POST (admin): create a user
    pub const NEW_PATH: &str = "/new";
    /// POST (admin): change names, email and admin flag of a user
    pub const UPDATE_PATH: &str = "/update/{id}";
    /// POST (admin): set a new local password for a user
    pub const RESET_PASSWORD_PATH: &str = "/reset-password/{id}";
    /// POST (admin): block a user from logging in
    pub const DISABLE_PATH: &str = "/disable/{id}";
    /// POST (admin): lift the block of a user
    pub const ENABLE_PATH: &str = "/enable/{id}";
    /// POST (admin, or the user themselves): delete a user; resources they owned are handed over, left ownerless or deleted
    pub const DELETE_PATH: &str = "/delete/{id}";
}

impl IsRouting for UsersRouting {
    const CONTROLLER_PATH: &str = "/user";
}

pub struct LoginRouting;
impl LoginRouting {
    pub const WITH_LOCAL: &str = "/with-local";
    /// Revokes all tokens of the authenticated user.
    pub const LOGOUT_PATH: &str = "/logout";
    pub const LOGIN_PROVIDERS_PATH: &str = "/login-providers";
    pub const WITH_OPEN_ID_CONNECT: &str = "/with-oidc/{name}";
}

impl IsRouting for LoginRouting {
    const CONTROLLER_PATH: &str = "/login";
}

pub struct ProjectsRouting;
impl ProjectsRouting {
    pub const INDEX_PATH: &str = "/index";
    pub const NEW_PATH: &str = "/new";
    pub const EDIT_PATH: &str = "/edit/{id}";
    pub const DELETE_PATH: &str = "/delete/{id}";
    pub const UPLOAD_FILE_PATH: &str = "/upload-file/{id}";
    pub const CREATE_FOLDER_PATH: &str = "/create-folder/{id}";
    pub const DELETE_PATH_PATH: &str = "/delete-path/{id}";
}

impl IsRouting for ProjectsRouting {
    const CONTROLLER_PATH: &str = "/projects";
}

/// Routes of the global reference data, a project-like singleton. The `{id}` segment is
/// [`ReferenceRouting::SINGLETON_ID`]; it exists so the routes fit the shared authorization
/// extractor.
pub struct ReferenceRouting;
impl ReferenceRouting {
    /// Id of the only reference data row
    pub const SINGLETON_ID: i64 = 1;
    /// GET: files and folders below the query parameter `subpath`
    pub const EDIT_PATH: &str = "/edit/{id}";
    /// POST (Editor): upload a chunk of a file
    pub const UPLOAD_FILE_PATH: &str = "/upload-file/{id}";
    /// POST (Editor): create a folder
    pub const CREATE_FOLDER_PATH: &str = "/create-folder/{id}";
    /// POST (Editor): delete a file or folder
    pub const DELETE_PATH_PATH: &str = "/delete-path/{id}";
}

impl IsRouting for ReferenceRouting {
    const CONTROLLER_PATH: &str = "/reference";
}

pub struct WorkflowIntegrationsRouting;
impl WorkflowIntegrationsRouting {
    pub const INDEX_PATH: &str = "/index";
    pub const NEW_PATH: &str = "/new";
    pub const EDIT_PATH: &str = "/edit/{id}";
    pub const DELETE_PATH: &str = "/delete/{id}";
    pub const UPDATE_PATH: &str = "/update/{id}";
    /// GET: schemas (`WorkflowSchemasResponse`) of the workflow, read from the cached checkout of
    /// a remote source or the local source; may fetch the repository
    pub const SCHEMAS_PATH: &str = "/schemas/{id}";
    /// GET (Editor): state of the background download of the workflow's remote repository
    /// (`PrefetchResponse`), started by `UPDATE_PATH`
    pub const PREFETCH_PATH: &str = "/prefetch/{id}";
    /// POST: replace the files of a local workflow by the zip archive in the request body (query
    /// parameter `script` optionally sets the main script); returns the updated workflow
    pub const UPLOAD_PATH: &str = "/upload/{id}";
}

impl IsRouting for WorkflowIntegrationsRouting {
    const CONTROLLER_PATH: &str = "/workflow-integrations";
}

/// Routes for runs
pub struct RunsRouting;
impl RunsRouting {
    /// POST: queue a run on a project
    pub const START_PATH: &str = "/start/{project_id}";
    /// GET: runs of a project, newest first
    pub const INDEX_PATH: &str = "/index/{project_id}";
    /// GET: one run
    pub const GET_PATH: &str = "/get/{id}";
    /// POST: cancel a queued run or ask a worker to stop a running one
    pub const CANCEL_PATH: &str = "/cancel/{id}";
    /// GET: log chunks of a run after the query parameter `after` (a chunk `seq`)
    pub const LOGS_PATH: &str = "/logs/{id}";
    /// GET: newest run of a project that still has a workflow, optionally only for the query
    /// parameter `workflow_id`; its parameter values prefill the start dialog
    pub const LAST_PATH: &str = "/last/{project_id}";
}

impl IsRouting for RunsRouting {
    const CONTROLLER_PATH: &str = "/runs";
}

/// Routes workers call. `claim` is authenticated with the worker credential (Basic), everything
/// else with the per-run token returned by `claim` (`Run <token>`).
pub struct WorkerRouting;
impl WorkerRouting {
    /// POST: lease the oldest queued run, 204 if none is queued
    pub const CLAIM_PATH: &str = "/claim";
    /// POST: announce runtime and engines (Basic); repeated so the backend knows the worker is alive
    pub const ANNOUNCE_PATH: &str = "/announce";
    /// POST: extend the lease, the response says whether the run should be cancelled
    pub const HEARTBEAT_PATH: &str = "/heartbeat/{run_id}";
    /// POST: append log lines of the run
    pub const LOGS_PATH: &str = "/logs/{run_id}";
    /// POST: report the task progress of the engine
    pub const PROGRESS_PATH: &str = "/progress/{run_id}";
    /// POST: report how the run ended; the token stops working afterwards
    pub const FINISH_PATH: &str = "/finish/{run_id}";
}

impl IsRouting for WorkerRouting {
    const CONTROLLER_PATH: &str = "/worker";
}

/// Routes for groups of users
pub struct GroupsRouting;
impl GroupsRouting {
    /// GET: groups the user owns or is a member of
    pub const INDEX_PATH: &str = "/index";
    /// POST: create a group owned by the user
    pub const NEW_PATH: &str = "/new";
    /// GET: one group with its members
    pub const EDIT_PATH: &str = "/edit/{id}";
    /// POST: rename a group or change its description, owner only
    pub const UPDATE_PATH: &str = "/update/{id}";
    /// POST: delete a group and the shares granted to it, owner only
    pub const DELETE_PATH: &str = "/delete/{id}";
    /// POST: add a member, owner only
    pub const ADD_MEMBER_PATH: &str = "/add-member/{id}";
    /// POST: remove a member, owner only
    pub const REMOVE_MEMBER_PATH: &str = "/remove-member/{id}";
}

impl IsRouting for GroupsRouting {
    const CONTROLLER_PATH: &str = "/groups";
}

/// Routes for the shares of projects and workflows; `{kind}` is `projects` or `workflows`
pub struct SharesRouting;
impl SharesRouting {
    /// GET: all shares of a resource, owners only
    pub const LIST_PATH: &str = "/{kind}/{id}";
    /// POST: grant a role to a principal or replace its role, owners only
    pub const GRANT_PATH: &str = "/{kind}/{id}/grant";
    /// POST: remove the grant of a principal, owners only
    pub const REVOKE_PATH: &str = "/{kind}/{id}/revoke";
}

impl IsRouting for SharesRouting {
    const CONTROLLER_PATH: &str = "/shares";
}
