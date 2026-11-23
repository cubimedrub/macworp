use dioxus::prelude::*;

use crate::router::IsRouter;

#[component]
pub fn OnePane<R: IsRouter>() -> Element {
    rsx!(
        main {
            class: "min-vh-100 conainer-fluid p-1",
            Outlet::<R> {}
        }
    )
}
