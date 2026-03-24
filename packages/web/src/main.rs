// mod embedded;
mod pages;
mod router;
mod storage;

use dioxus::prelude::*;
use thiserror::Error;
use web_sys::window;

use macworp_ui_core::{components::Bootstrap, configuration::Configuration, FAVICON, MAIN_CSS};

use crate::{router::Router, storage::Storage};

#[derive(Debug, Error)]
pub enum Error {
    #[error("Failed to aquire document window from browser")]
    Window,
    #[error("Failed to get host from the browser location. Without it information the application do not now where to send requests: {0:?}")]
    Host(web_sys::wasm_bindgen::JsValue),
    #[error("Failed to get protocol from the browser location. Without it the application do not now which protocol to use to send requests: {0:?}")]
    Protocol(web_sys::wasm_bindgen::JsValue),
    #[error("{0}")]
    Storage(#[from] crate::storage::Error),
}

fn main() {
    // The `launch` function is the main entry point for a dioxus app. It takes a component and renders it with the platform feature
    // you have enabled
    dioxus::launch(App);
}

fn create_config() -> Result<Configuration, Error> {
    let base_url_override: Option<&'static str> = option_env!("MACWORP_BASE_URL");

    if let Some(base_url) = base_url_override {
        return Ok(Configuration::new(base_url.to_string(), 60));
    }

    let location = window().ok_or(Error::Window)?.location();

    let host = location.host().map_err(Error::Host)?;

    let protocol = location.protocol().map_err(Error::Protocol)?;

    Ok(Configuration::new(format!("{}//{}", protocol, host), 60))
}

#[component]
fn App() -> Element {
    let config_res: Result<(), Error> = match create_config() {
        Ok(config) => {
            use_context_provider(|| config);
            Ok(())
        }
        Err(e) => Err(e),
    };

    let storage_res: Result<(), Error> = match Storage::new() {
        Ok(storage) => {
            use_context_provider(|| storage);
            Ok(())
        }
        Err(e) => Err(e.into()),
    };

    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        Bootstrap {}

        if config_res.is_ok() && storage_res.is_ok() {
            Router::<Router> {}
        } else {
            if let Err(config) = config_res {
                div { class: "alert alert-danger", role: "alert", "Failed to load configuration: {config}" }
            }
            if let Err(storage) = storage_res {
                div { class: "alert alert-danger", role: "alert", "Failed to initialize storage: {storage}" }
            }
        }
    }
}
