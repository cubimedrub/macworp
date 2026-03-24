use paste::paste;

pub trait IsRouting {
    const CONTROLLER_PATH: &str;

    fn controller_path() -> &'static str {
        Self::CONTROLLER_PATH
    }
}

/// Creates a simple Routing struct for one controller to centralize the path segments of the URL
#[macro_export]
macro_rules! routing {
    ($name:ident, $( $path:literal ),+) => {
        paste! {
            pub struct [< $name Routing >];

            impl [<$name Routing>] {
                $(
                    pub fn [< $path:snake _path >]() -> &'static str {
                        concat!("/", stringify!([< $path:lower >]))
                    }
                )+
            }

            impl IsRouting for [< $name Routing >] {
                const CONTROLLER_PATH: &str = concat!("/", stringify!([< $name:lower >]));
            }
        }
    };
}

routing!(Utilities, "healthcheck", "version");

routing!(Users, "current");

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
}

impl IsRouting for ProjectsRouting {
    const CONTROLLER_PATH: &str = "/projects";
}
