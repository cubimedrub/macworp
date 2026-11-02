use dioxus::prelude::*;
use macworp_core::responses::UserResponse;

use crate::{
    components::Spinner, configuration::Configuration, router::IsRouter, session::Session,
    storage::IsStorage,
};

/// Lists all users with a username filter; for administrators.
#[component]
pub fn Index<S: IsStorage + 'static, R: IsRouter>() -> Element {
    let storage = use_context::<Signal<S>>();
    let config = use_context::<Signal<Configuration>>();

    let mut query = use_signal(String::new);

    let mut users: Resource<Result<Vec<UserResponse>, String>> = use_resource(move || async move {
        let query = query();
        let session = Session::new(storage, config)?;
        session
            .client
            .users()
            .index(&session.jwt, query.trim())
            .await
            .map_err(|e| e.to_string())
    });

    rsx! {
        div {
            class: "d-flex justify-content-between mb-3",
            h1 { "Users" }
            Link {
                class: "btn btn-primary h-fit-content",
                to: R::user_new_route(),
                i { class: "fas fa-plus" }
            }
        }
        input {
            class: "form-control mb-3",
            placeholder: "Filter by username",
            aria_label: "Filter by username",
            value: "{query}",
            oninput: move |event| query.set(event.value()),
        }
        match &*users.read() {
            Some(Ok(users)) => rsx! {
                ul {
                    class: "list-group",
                    for user in users {
                        Link {
                            class: "list-group-item",
                            to: R::user_route(user.id),
                            "{user.username}"
                            span { class: "text-muted ms-2", "{user.firstname} {user.lastname}" }
                            if user.is_admin {
                                span { class: "badge text-bg-primary ms-2", "admin" }
                            }
                            if user.disabled {
                                span { class: "badge text-bg-secondary ms-2", "disabled" }
                            }
                        }
                    }
                }
            },
            Some(Err(err)) => rsx! {
                div {
                    class: "alert alert-danger",
                    ondoubleclick: move |_| users.restart(),
                    "{err}"
                }
            },
            None => rsx! { Spinner {} },
        }
    }
}
