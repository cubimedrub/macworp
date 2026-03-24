use dioxus::{logger::tracing, prelude::*};
use macworp_core::responses::LoginProviderResponse;
use macworp_ui_core::{client::Client, components::Spinner, configuration::Configuration};

use crate::router::Router;

fn get_login_url(provider: &LoginProviderResponse) -> NavigationTarget<Router> {
    match provider {
        LoginProviderResponse::Local(_) => NavigationTarget::Internal(Router::LocalLogin),
        LoginProviderResponse::OpenIdConnect {
            name,
            description: _,
        } => {
            let config = use_context::<Configuration>();
            NavigationTarget::External(
                Client::new(config.base_url().to_string(), config.timeout())
                    .login()
                    .open_id_connect_url(name),
            )
        }
    }
}

#[derive(Clone, PartialEq, Props)]
pub struct LoginProps {
    pub message: Option<String>,
}

#[component]
pub fn Login(props: LoginProps) -> Element {
    let login_providers = use_resource(async || {
        let config = use_context::<Configuration>();
        tracing::info!("Fetching login providers from {}", config.base_url());
        Client::new(config.base_url().to_string(), config.timeout())
            .login()
            .login_providers()
            .await
    });

    rsx! {
        h1 {
            "Login"
        }
        if let Some(message) = &props.message {
            div {
                class: "alert alert-info",
                "{message}"
            }
        }
        match &*login_providers.read() {
            Some(Ok(providers)) => rsx! {
                div {
                    class: "row justify-content-center login",
                    for provider in providers {
                        div {
                            class: "card m-2",
                            style: "width: 18rem;",
                            div {
                                class: "card-body",
                                h5 {
                                    class: "card-title",
                                    "{provider}"
                                }
                                p {
                                    class: "card-text",
                                    {provider.description()}
                                }
                                Link {
                                    class: "btn btn-primary",
                                    to: get_login_url(provider),
                                    "login"
                                }
                            }
                        }
                    }
                }
            },
            Some(Err(err)) => rsx! {
                div {
                    class: "alert alert-danger",
                    role: "alert",
                    "Failed to load login providers: {err}"
                }
            },
            None => rsx! {
                Spinner {}
            }
        }
    }
}
