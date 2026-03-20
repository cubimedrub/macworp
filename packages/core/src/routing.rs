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
routing!(Login, "with_password");
routing!(Users, "current");

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
