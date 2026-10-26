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
    pub const CURRENT_PATH: &str = "/current";
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
}

impl IsRouting for RunsRouting {
    const CONTROLLER_PATH: &str = "/runs";
}
