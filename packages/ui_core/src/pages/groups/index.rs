use dioxus::prelude::*;
use macworp_core::responses::GroupResponse;

use crate::{
    components::{OwnerlessBadge, ShowAll, ShowAllToggle, Spinner},
    configuration::Configuration,
    router::IsRouter,
    session::Session,
    storage::IsStorage,
};

/// Lists the groups the user owns or belongs to.
#[component]
pub fn Index<S: IsStorage + 'static, R: IsRouter>() -> Element {
    let storage = use_context::<Signal<S>>();
    let config = use_context::<Signal<Configuration>>();

    let show_all = ShowAll::<S>::from_context();
    let mut groups: Resource<Result<Vec<GroupResponse>, String>> =
        use_resource(move || async move {
            let session = Session::new(storage, config)?;
            let all = show_all.is_active();
            session
                .client
                .groups()
                .index(&session.jwt, all)
                .await
                .map_err(|e| e.to_string())
        });

    rsx! {
        div {
            class: "d-flex justify-content-between mb-3",
            h1 { "Groups" }
            div {
                class: "d-flex",
                ShowAllToggle::<S> {}
                Link {
                    class: "btn btn-primary h-fit-content",
                    to: R::group_new_route(),
                    i { class: "fas fa-plus" }
                }
            }
        }
        match &*groups.read() {
            Some(Ok(groups)) => rsx! {
                ul {
                    class: "list-group",
                    for group in groups {
                        Link {
                            class: "list-group-item",
                            to: R::group_route(group.id),
                            "{group.name}"
                            if group.ownerless {
                                OwnerlessBadge {}
                            } else if !group.is_owner {
                                span { class: "badge text-bg-secondary ms-2", "owner: {group.owner_username}" }
                            }
                        }
                    }
                }
            },
            Some(Err(err)) => rsx! {
                div {
                    class: "alert alert-danger",
                    ondoubleclick: move |_| groups.restart(),
                    "{err}"
                }
            },
            None => rsx! { Spinner {} },
        }
    }
}
