use dioxus::prelude::*;
use macworp_core::{
    requests::{ResetPasswordRequest, UpdateUserRequest},
    responses::UserResponse,
};

use crate::{
    components::{AccountDeletionRules, ConfirmationDialog, Spinner},
    configuration::Configuration,
    router::IsRouter,
    session::Session,
    storage::IsStorage,
};

#[derive(Clone, PartialEq, Props)]
pub struct EditProps {
    /// Id of the user
    pub id: i64,
}

/// A user for administrators: change names, email and admin flag, reset the password, block or
/// unblock the account and delete it.
#[component]
pub fn Edit<S: IsStorage + 'static, R: IsRouter + 'static>(props: EditProps) -> Element {
    let storage = use_context::<Signal<S>>();
    let config = use_context::<Signal<Configuration>>();
    let navigator = navigator();

    let mut is_delete_dialog_open = use_signal(|| false);
    let mut error: Signal<Option<String>> = use_signal(|| None);
    let mut firstname = use_signal(String::new);
    let mut lastname = use_signal(String::new);
    let mut email = use_signal(String::new);
    let mut is_admin = use_signal(|| false);
    let mut new_password = use_signal(String::new);

    let mut user: Resource<Result<UserResponse, String>> = use_resource(move || async move {
        let session = Session::new(storage, config)?;
        // There is no single-user endpoint, the list filtered by id is the source
        let user = session
            .client
            .users()
            .index(&session.jwt, "")
            .await
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|user| user.id == props.id)
            .ok_or_else(|| "User not found".to_string())?;
        firstname.set(user.firstname.clone());
        lastname.set(user.lastname.clone());
        email.set(user.email.clone());
        is_admin.set(user.is_admin);
        Ok(user)
    });

    let save = move || {
        spawn(async move {
            let request = UpdateUserRequest {
                firstname: firstname.read().trim().to_string(),
                lastname: lastname.read().trim().to_string(),
                email: email.read().trim().to_string(),
                is_admin: is_admin(),
            };
            let result = match Session::new(storage, config) {
                Ok(session) => session
                    .client
                    .users()
                    .update(&session.jwt, props.id, &request)
                    .await
                    .map_err(|e| e.to_string())
                    .map(|_| ()),
                Err(e) => Err(e),
            };
            match result {
                Ok(()) => {
                    error.set(None);
                    user.restart();
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    let reset_password = move || {
        spawn(async move {
            let request = ResetPasswordRequest {
                password: new_password.read().clone(),
            };
            let result = match Session::new(storage, config) {
                Ok(session) => session
                    .client
                    .users()
                    .reset_password(&session.jwt, props.id, &request)
                    .await
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e),
            };
            match result {
                Ok(()) => {
                    error.set(None);
                    new_password.set(String::new());
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    let set_disabled = move |disabled: bool| {
        spawn(async move {
            let result = match Session::new(storage, config) {
                Ok(session) => {
                    let users = session.client.users();
                    if disabled {
                        users.disable(&session.jwt, props.id).await
                    } else {
                        users.enable(&session.jwt, props.id).await
                    }
                    .map_err(|e| e.to_string())
                    .map(|_| ())
                }
                Err(e) => Err(e),
            };
            match result {
                Ok(()) => {
                    error.set(None);
                    user.restart();
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    let delete = move |_| {
        is_delete_dialog_open.set(false);
        spawn(async move {
            let result = match Session::new(storage, config) {
                Ok(session) => session
                    .client
                    .users()
                    .delete(&session.jwt, props.id)
                    .await
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e),
            };
            match result {
                Ok(()) => {
                    navigator.push(R::user_index_route());
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    rsx! {
        ConfirmationDialog {
            is_open: is_delete_dialog_open,
            title: "Delete user".to_string(),
            confirmation_button_label: "Delete".to_string(),
            confirmation_callback: Callback::new(delete),
            cancel_callback: Callback::new(move |_| is_delete_dialog_open.set(false)),
            AccountDeletionRules {}
        }
        if let Some(message) = error() {
            div { class: "alert alert-danger", role: "alert", "{message}" }
        }
        match &*user.read() {
            Some(Ok(loaded)) => {
                let disabled = loaded.disabled;
                rsx! {
                    div {
                        class: "d-flex flex-row justify-content-between mb-3",
                        h1 {
                            class: "mb-0",
                            "{loaded.username}"
                            if disabled {
                                span { class: "badge text-bg-secondary ms-2 fs-6", "disabled" }
                            }
                        }
                        div {
                            class: "d-flex flex-row",
                            button {
                                class: "btn btn-sm btn-outline-primary me-2",
                                onclick: move |_| save(),
                                i { class: "fa-solid fa-floppy-disk me-1" }
                                "Save"
                            }
                            button {
                                class: "btn btn-sm btn-outline-secondary me-2",
                                onclick: move |_| set_disabled(!disabled),
                                if disabled {
                                    i { class: "fa-solid fa-lock-open me-1" }
                                    "Enable"
                                } else {
                                    i { class: "fa-solid fa-ban me-1" }
                                    "Disable"
                                }
                            }
                            button {
                                class: "btn btn-sm btn-outline-danger",
                                onclick: move |_| is_delete_dialog_open.set(true),
                                i { class: "fa-solid fa-trash me-1" }
                                "Delete"
                            }
                        }
                    }
                    div {
                        class: "input-group mb-3",
                        span { class: "input-group-text", "First name" }
                        input {
                            r#type: "text",
                            class: "form-control",
                            aria_label: "First name",
                            value: "{firstname}",
                            oninput: move |event| firstname.set(event.value()),
                            onkeyup: move |event| {
                                if event.code() == Code::Enter { save(); }
                            },
                        }
                    }
                    div {
                        class: "input-group mb-3",
                        span { class: "input-group-text", "Last name" }
                        input {
                            r#type: "text",
                            class: "form-control",
                            aria_label: "Last name",
                            value: "{lastname}",
                            oninput: move |event| lastname.set(event.value()),
                            onkeyup: move |event| {
                                if event.code() == Code::Enter { save(); }
                            },
                        }
                    }
                    div {
                        class: "input-group mb-3",
                        span { class: "input-group-text", "Email" }
                        input {
                            r#type: "email",
                            class: "form-control",
                            aria_label: "Email",
                            value: "{email}",
                            oninput: move |event| email.set(event.value()),
                            onkeyup: move |event| {
                                if event.code() == Code::Enter { save(); }
                            },
                        }
                    }
                    div {
                        class: "form-check mb-4",
                        input {
                            r#type: "checkbox",
                            class: "form-check-input",
                            id: "edit-user-is-admin",
                            checked: is_admin(),
                            onchange: move |event| is_admin.set(event.checked()),
                        }
                        label { class: "form-check-label", r#for: "edit-user-is-admin", "Administrator" }
                    }
                    h2 { "Reset password" }
                    div {
                        class: "input-group mb-3",
                        input {
                            r#type: "password",
                            class: "form-control",
                            aria_label: "New password",
                            placeholder: "New password",
                            autocomplete: "new-password",
                            value: "{new_password}",
                            oninput: move |event| new_password.set(event.value()),
                            onkeyup: move |event| {
                                if event.code() == Code::Enter && !new_password.read().is_empty() {
                                    reset_password();
                                }
                            },
                        }
                        button {
                            class: "btn btn-outline-primary",
                            disabled: new_password.read().is_empty(),
                            onclick: move |_| reset_password(),
                            "Set password"
                        }
                    }
                }
            },
            Some(Err(err)) => rsx! {
                div { class: "alert alert-danger", ondoubleclick: move |_| user.restart(), "{err}" }
            },
            None => rsx! { Spinner {} },
        }
    }
}
