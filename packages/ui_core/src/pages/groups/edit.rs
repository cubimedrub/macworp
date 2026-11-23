use dioxus::prelude::*;
use macworp_core::responses::{GroupResponse, UserSummaryResponse};

use crate::{
    components::{ConfirmationDialog, OwnerlessBadge, Spinner},
    configuration::Configuration,
    router::IsRouter,
    session::Session,
    storage::IsStorage,
};

#[derive(Clone, PartialEq, Props)]
pub struct EditProps {
    /// Id of the group
    pub id: i64,
}

/// A group with its members. Members see it read-only; the owner can rename it, add and remove
/// members and delete it.
#[component]
pub fn Edit<S: IsStorage + 'static, R: IsRouter + 'static>(props: EditProps) -> Element {
    let storage = use_context::<Signal<S>>();
    let config = use_context::<Signal<Configuration>>();
    let navigator = navigator();

    let mut is_delete_dialog_open = use_signal(|| false);
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let mut name = use_signal(String::new);
    let mut description = use_signal(String::new);
    let mut member_query = use_signal(String::new);

    let mut group: Resource<Result<GroupResponse, String>> = use_resource(move || async move {
        let session = Session::new(storage, config)?;
        let group = session
            .client
            .groups()
            .group(&session.jwt, props.id)
            .await
            .map_err(|e| e.to_string())?;
        name.set(group.name.clone());
        description.set(group.description.clone());
        Ok(group)
    });

    let found_users: Resource<Vec<UserSummaryResponse>> = use_resource(move || async move {
        let query = member_query();
        if query.trim().chars().count() < 2 {
            return Vec::new();
        }
        let Ok(session) = Session::new(storage, config) else {
            return Vec::new();
        };
        session
            .client
            .users()
            .search(&session.jwt, query.trim())
            .await
            .unwrap_or_default()
    });

    let save = move || {
        spawn(async move {
            let (name, description) = (name.read().clone(), description.read().clone());
            let result = match Session::new(storage, config) {
                Ok(session) => session
                    .client
                    .groups()
                    .update(&session.jwt, props.id, name, description)
                    .await
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e),
            };
            match result {
                Ok(_) => {
                    error.set(None);
                    group.restart();
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    let mut delete = move |_| {
        is_delete_dialog_open.set(false);
        spawn(async move {
            let result = match Session::new(storage, config) {
                Ok(session) => session
                    .client
                    .groups()
                    .delete(&session.jwt, props.id)
                    .await
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e),
            };
            match result {
                Ok(()) => {
                    navigator.push(R::group_index_route());
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    let add_member = move |user_id: i64| {
        spawn(async move {
            let result = match Session::new(storage, config) {
                Ok(session) => session
                    .client
                    .groups()
                    .add_member(&session.jwt, props.id, user_id)
                    .await
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e),
            };
            match result {
                Ok(_) => {
                    error.set(None);
                    member_query.set(String::new());
                    group.restart();
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    let remove_member = move |user_id: i64| {
        spawn(async move {
            let result = match Session::new(storage, config) {
                Ok(session) => session
                    .client
                    .groups()
                    .remove_member(&session.jwt, props.id, user_id)
                    .await
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e),
            };
            match result {
                Ok(_) => {
                    error.set(None);
                    group.restart();
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    rsx! {
        ConfirmationDialog {
            is_open: is_delete_dialog_open,
            title: "Delete group".to_string(),
            body: "Are you sure you want to delete this group? Everything shared with it stops being accessible through it. This action cannot be undone.".to_string(),
            confirmation_button_label: "Delete".to_string(),
            confirmation_callback: Callback::new(move |_| delete(())),
            cancel_callback: Callback::new(move |_| is_delete_dialog_open.set(false)),
        }
        if let Some(message) = error() {
            div { class: "alert alert-danger", role: "alert", "{message}" }
        }
        match &*group.read() {
            Some(Ok(group)) => {
                let is_owner = group.is_owner;
                let members = group.members.clone();
                let owner_username = group.owner_username.clone();
                rsx! {
                    div {
                        class: "d-flex flex-row justify-content-between mb-3",
                        h1 {
                            class: "mb-0",
                            "{group.name}"
                            if group.ownerless {
                                OwnerlessBadge {}
                            }
                        }
                        if is_owner {
                            div {
                                class: "d-flex flex-row",
                                button {
                                    class: "btn btn-sm btn-outline-primary me-2",
                                    onclick: move |_| save(),
                                    i { class: "fa-solid fa-floppy-disk me-1" }
                                    "Save"
                                }
                                button {
                                    class: "btn btn-sm btn-outline-danger",
                                    onclick: move |_| is_delete_dialog_open.set(true),
                                    i { class: "fa-solid fa-trash me-1" }
                                    "Delete"
                                }
                            }
                        }
                    }
                    p { class: "text-muted", "Owner: {owner_username}" }
                    div {
                        class: "input-group mb-3",
                        span { class: "input-group-text", "Name" }
                        input {
                            r#type: "text",
                            class: "form-control",
                            readonly: !is_owner,
                            value: "{name}",
                            oninput: move |event| name.set(event.value()),
                            onkeyup: move |event| {
                                if is_owner && event.code() == Code::Enter && !name.read().trim().is_empty() {
                                    save();
                                }
                            },
                        }
                    }
                    div {
                        class: "input-group mb-3",
                        span { class: "input-group-text", "Description" }
                        textarea {
                            class: "form-control",
                            readonly: !is_owner,
                            value: "{description}",
                            oninput: move |event| description.set(event.value()),
                        }
                    }
                    h2 { "Members" }
                    ul {
                        class: "list-group mb-3",
                        for member in members {
                            li {
                                class: "list-group-item d-flex align-items-center",
                                span { class: "flex-fill", "{member.username}" }
                                if is_owner && member.username != owner_username {
                                    button {
                                        class: "btn btn-sm btn-outline-danger",
                                        title: "Double-click to remove member",
                                        ondoubleclick: move |_| remove_member(member.id),
                                        i { class: "fa-solid fa-xmark" }
                                    }
                                }
                            }
                        }
                    }
                    if is_owner {
                        input {
                            class: "form-control form-control-sm",
                            placeholder: "Add member: search username (min. 2 characters)",
                            value: "{member_query}",
                            oninput: move |evt| member_query.set(evt.value()),
                        }
                        ul {
                            class: "list-group",
                            for user in found_users.read().clone().unwrap_or_default() {
                                button {
                                    class: "list-group-item list-group-item-action py-1",
                                    onclick: move |_| add_member(user.id),
                                    "{user.username}"
                                }
                            }
                        }
                    }
                }
            },
            Some(Err(err)) => rsx! {
                div { class: "alert alert-danger", ondoubleclick: move |_| group.restart(), "{err}" }
            },
            None => rsx! { Spinner {} },
        }
    }
}
