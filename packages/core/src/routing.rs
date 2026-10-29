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
}

impl IsRouting for UsersRouting {
    const CONTROLLER_PATH: &str = "/user";
}

pub struct LoginRouting;
impl LoginRouting {
    pub const WITH_LOCAL: &str = "/with-local";
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

pub struct WorkflowIntegrationsRouting;
impl WorkflowIntegrationsRouting {
    pub const INDEX_PATH: &str = "/index";
    pub const NEW_PATH: &str = "/new";
    pub const EDIT_PATH: &str = "/edit/{id}";
    pub const DELETE_PATH: &str = "/delete/{id}";
    pub const UPDATE_PATH: &str = "/update/{id}";
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
