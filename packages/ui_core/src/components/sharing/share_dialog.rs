use dioxus::prelude::*;
use macworp_core::{
    entities::Role,
    requests::PrincipalRequest,
    responses::{GroupResponse, PrincipalResponse, ShareResponse, UserSummaryResponse},
};

use crate::{
    client::ShareKind, configuration::Configuration, session::Session, storage::IsStorage,
};

/// Role named by a select option value, `Viewer` for unknown values.
fn role_from_value(value: &str) -> Role {
    match value {
        "Owner" => Role::Owner,
        "Editor" => Role::Editor,
        _ => Role::Viewer,
    }
}

/// Roles a principal may hold: all of them, except for the public which is capped by the kind.
fn roles_for(kind: ShareKind, is_public: bool) -> Vec<Role> {
    [Role::Viewer, Role::Editor, Role::Owner]
        .into_iter()
        .filter(|role| !is_public || *role <= kind.max_public_role())
        .collect()
}

/// Who a new share is granted to, as chosen in the form
#[derive(Clone, Copy, PartialEq, Eq)]
enum TargetKind {
    /// A user picked from the search results
    User,
    /// One of the user's groups
    Group,
    /// Every user
    Public,
}

#[derive(Clone, PartialEq, Props)]
pub struct ShareDialogProps {
    /// Whether a project or a workflow is shared
    pub kind: ShareKind,
    /// Id of the shared project or workflow
    pub resource_id: i64,
    pub is_open: ReadSignal<bool>,
    pub cancel_callback: Callback<(), ()>,
}

/// Dialog listing who holds a role on a resource and letting its owners change that: grant a role
/// to a user, a group or everybody, change roles and remove grants.
///
/// # Arguments
/// * `kind` - Whether a project or a workflow is shared
/// * `resource_id` - Id of the shared resource
/// * `is_open` - A signal that indicates whether the dialog is open or not
/// * `cancel_callback` - The callback to call when the dialog is closed
#[component]
pub fn ShareDialog<S: IsStorage + 'static>(props: ShareDialogProps) -> Element {
    let storage = use_context::<Signal<S>>();
    let config = use_context::<Signal<Configuration>>();

    // Counts changes so the share list reloads
    let mut version = use_signal(|| 0u32);
    let mut error: Signal<Option<String>> = use_signal(|| None);

    let mut target_kind = use_signal(|| TargetKind::User);
    let mut new_role = use_signal(|| Role::Viewer);
    let mut user_query = use_signal(String::new);
    let mut picked_user: Signal<Option<UserSummaryResponse>> = use_signal(|| None);
    let mut picked_group: Signal<Option<i64>> = use_signal(|| None);

    use_effect(move || {
        if *props.is_open.read() {
            error.set(None);
            user_query.set(String::new());
            picked_user.set(None);
            picked_group.set(None);
            version += 1;
        }
    });

    let shares: Resource<Result<Vec<ShareResponse>, String>> = use_resource(move || async move {
        let _ = version();
        if !*props.is_open.read() {
            return Ok(Vec::new());
        }
        let session = Session::new(storage, config)?;
        session
            .client
            .shares()
            .list(&session.jwt, props.kind, props.resource_id)
            .await
            .map_err(|e| e.to_string())
    });

    let groups: Resource<Result<Vec<GroupResponse>, String>> = use_resource(move || async move {
        if !*props.is_open.read() {
            return Ok(Vec::new());
        }
        let session = Session::new(storage, config)?;
        session
            .client
            .groups()
            .index(&session.jwt)
            .await
            .map_err(|e| e.to_string())
    });

    let found_users: Resource<Vec<UserSummaryResponse>> = use_resource(move || async move {
        let query = user_query();
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

    let grant = move |principal: PrincipalRequest, role: Role| {
        spawn(async move {
            let result = match Session::new(storage, config) {
                Ok(session) => session
                    .client
                    .shares()
                    .grant(&session.jwt, props.kind, props.resource_id, principal, role)
                    .await
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e),
            };
            match result {
                Ok(()) => {
                    error.set(None);
                    version += 1;
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    let revoke = move |principal: PrincipalRequest| {
        spawn(async move {
            let result = match Session::new(storage, config) {
                Ok(session) => session
                    .client
                    .shares()
                    .revoke(&session.jwt, props.kind, props.resource_id, principal)
                    .await
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e),
            };
            match result {
                Ok(()) => {
                    error.set(None);
                    version += 1;
                }
                Err(e) => error.set(Some(e)),
            }
        });
    };

    let modal_class = use_memo(move || {
        if *props.is_open.read() {
            "modal show"
        } else {
            "modal"
        }
    });
    let modal_style = use_memo(move || {
        if *props.is_open.read() {
            "display: block;"
        } else {
            "display: none;"
        }
    });

    // The pending selection of the add form, if complete
    let selected_principal = move || match target_kind() {
        TargetKind::User => picked_user
            .read()
            .as_ref()
            .map(|u| PrincipalRequest::User(u.id)),
        TargetKind::Group => picked_group().map(PrincipalRequest::Group),
        TargetKind::Public => Some(PrincipalRequest::Public),
    };
    let add_roles = roles_for(props.kind, target_kind() == TargetKind::Public);
    // A role chosen for a user may be too high for the public, so fall back to the highest valid
    let effective_new_role = if add_roles.contains(&new_role()) {
        new_role()
    } else {
        *add_roles.last().unwrap_or(&Role::Viewer)
    };

    rsx! {
        div {
            class: modal_class,
            tabindex: "-1",
            style: modal_style,
            div {
                class: "modal-dialog modal-lg",
                div {
                    class: "modal-content",
                    div {
                        class: "modal-header",
                        h5 { class: "modal-title", "Share" }
                        button {
                            r#type: "button",
                            class: "btn-close",
                            aria_label: "Close",
                            onclick: move |_| props.cancel_callback.call(()),
                        }
                    }
                    div {
                        class: "modal-body",
                        if let Some(message) = error() {
                            div { class: "alert alert-danger", role: "alert", "{message}" }
                        }
                        match &*shares.read() {
                            None => rsx! { p { "Loading..." } },
                            Some(Err(e)) => rsx! { div { class: "alert alert-danger", role: "alert", "{e}" } },
                            Some(Ok(entries)) => rsx! {
                                ul {
                                    class: "list-group mb-3",
                                    for entry in entries.clone() {
                                        {
                                            let (label, request, is_public) = match &entry.principal {
                                                PrincipalResponse::User { id, username } => (format!("User {username}"), PrincipalRequest::User(*id), false),
                                                PrincipalResponse::Group { id, name } => (format!("Group {name}"), PrincipalRequest::Group(*id), false),
                                                PrincipalResponse::Public => ("Everybody (public)".to_string(), PrincipalRequest::Public, true),
                                            };
                                            rsx! {
                                                li {
                                                    class: "list-group-item d-flex align-items-center",
                                                    span { class: "flex-fill", "{label}" }
                                                    select {
                                                        class: "form-select form-select-sm w-auto me-2",
                                                        value: "{entry.role:?}",
                                                        onchange: move |evt| grant(request, role_from_value(&evt.value())),
                                                        for role in roles_for(props.kind, is_public) {
                                                            option { value: "{role:?}", selected: role == entry.role, "{role:?}" }
                                                        }
                                                    }
                                                    button {
                                                        class: "btn btn-sm btn-outline-danger",
                                                        title: "Double-click to remove access",
                                                        ondoubleclick: move |_| revoke(request),
                                                        i { class: "fa-solid fa-xmark" }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            },
                        }
                        h6 { "Add" }
                        div {
                            class: "d-flex flex-row align-items-start",
                            select {
                                class: "form-select form-select-sm w-auto me-2",
                                aria_label: "Kind of principal",
                                onchange: move |evt| target_kind.set(match evt.value().as_str() {
                                    "group" => TargetKind::Group,
                                    "public" => TargetKind::Public,
                                    _ => TargetKind::User,
                                }),
                                option { value: "user", "User" }
                                option { value: "group", "Group" }
                                option { value: "public", "Everybody" }
                            }
                            match target_kind() {
                                TargetKind::User => rsx! {
                                    div {
                                        class: "flex-fill me-2",
                                        if let Some(user) = picked_user() {
                                            div {
                                                class: "form-control form-control-sm d-flex",
                                                span { class: "flex-fill", "{user.username}" }
                                                button {
                                                    class: "btn-close",
                                                    aria_label: "Clear",
                                                    onclick: move |_| picked_user.set(None),
                                                }
                                            }
                                        } else {
                                            input {
                                                class: "form-control form-control-sm",
                                                placeholder: "Search username (min. 2 characters)",
                                                value: "{user_query}",
                                                oninput: move |evt| user_query.set(evt.value()),
                                            }
                                            ul {
                                                class: "list-group",
                                                for user in found_users.read().clone().unwrap_or_default() {
                                                    {
                                                        let picked = user.clone();
                                                        rsx! {
                                                            button {
                                                                class: "list-group-item list-group-item-action py-1",
                                                                onclick: move |_| picked_user.set(Some(picked.clone())),
                                                                "{user.username}"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                },
                                TargetKind::Group => rsx! {
                                    select {
                                        class: "form-select form-select-sm flex-fill me-2",
                                        aria_label: "Group",
                                        onchange: move |evt| picked_group.set(evt.value().parse().ok()),
                                        option { value: "", "Select a group" }
                                        for group in groups.read().clone().and_then(Result::ok).unwrap_or_default() {
                                            option { value: "{group.id}", "{group.name}" }
                                        }
                                    }
                                },
                                TargetKind::Public => rsx! { div { class: "flex-fill" } },
                            }
                            select {
                                class: "form-select form-select-sm w-auto me-2",
                                aria_label: "Role",
                                value: "{effective_new_role:?}",
                                onchange: move |evt| new_role.set(role_from_value(&evt.value())),
                                for role in add_roles {
                                    option { value: "{role:?}", "{role:?}" }
                                }
                            }
                            button {
                                class: "btn btn-sm btn-primary",
                                disabled: selected_principal().is_none(),
                                onclick: move |_| {
                                    if let Some(principal) = selected_principal() {
                                        grant(principal, effective_new_role);
                                        picked_user.set(None);
                                        user_query.set(String::new());
                                    }
                                },
                                "Add"
                            }
                        }
                    }
                    div {
                        class: "modal-footer",
                        button {
                            r#type: "button",
                            class: "btn btn-secondary",
                            onclick: move |_| props.cancel_callback.call(()),
                            "Close"
                        }
                    }
                }
            }
        }
    }
}
