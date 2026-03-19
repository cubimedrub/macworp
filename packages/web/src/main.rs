mod storage;

use dioxus::prelude::*;

use macworp_ui_core::{configuration::Configuration, routes::Routes, FAVICON, MAIN_CSS};

use crate::storage::Storage;

fn main() {
    // The `launch` function is the main entry point for a dioxus app. It takes a component and renders it with the platform feature
    // you have enabled
    dioxus::launch(App);
}

async fn create_config() -> Configuration {
    let domain = match document::eval("return window.location.host").await {
        Ok(d) => d.to_string().replace('"', ""),
        Err(err) => {
            error!("Could not retrieve domain from window.location.host: {err}",);
            "".to_string()
        }
    };

    let scheme = match document::eval("return window.location.protocol").await {
        Ok(d) => d.to_string().replace('"', ""),
        Err(err) => {
            error!("Could not retrieve domain from window.location.host: {err}",);
            "http:".to_string()
        }
    };

    Configuration::new(format!("{}//{}", scheme, domain))
}

#[component]
fn App() -> Element {
    let config = use_resource(create_config);
    use_context_provider(|| config);

    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }


        // The router component renders the route enum we defined above. It will handle synchronization of the URL and render
        // the layouts and components for the active route.
        Router::<Routes> {}
    }
}
