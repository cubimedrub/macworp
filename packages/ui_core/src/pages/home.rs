use dioxus::prelude::*;

use crate::components::Hero;
use crate::configuration::Configuration;

/// The Home page component that will be rendered when the current route is `[Route::Home]`
#[component]
pub fn Home() -> Element {
    let config = use_context::<Resource<Configuration>>();
    let server_hello: Resource<String> = use_resource(move || async move {
        let base_url = match config.read_unchecked().as_ref() {
            Some(config) => config.base_url().to_string(),
            None => "https://couldnotfoundanyhting.localhost".to_string(),
        };

        reqwest::get(base_url).await.unwrap().text().await.unwrap()
    });

    rsx! {
        Hero {}

        if let Some(server_hello) = &*server_hello.read_unchecked() {
            div {
                "Server says: {server_hello}"
            }
        }
    }
}
