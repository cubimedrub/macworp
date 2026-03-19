use paste::paste;

pub trait IsRouting {
    fn controller_path() -> &'static str;
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
                fn controller_path() -> &'static str {
                    concat!("/", stringify!([< $name:lower >]))
                }
            }
        }
    };
}

routing!(Utilities, "healthcheck", "version");
routing!(Login, "with_password");
routing!(Users, "current");
