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
}

impl IsRouting for ProjectsRouting {
    const CONTROLLER_PATH: &str = "/projects";
}
