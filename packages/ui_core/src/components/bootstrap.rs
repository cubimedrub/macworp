use dioxus::{document::Script, prelude::*};

#[component]
pub fn Bootstrap() -> Element {
    rsx! {
        Stylesheet { href: "https://cdn.jsdelivr.net/npm/bootstrap@5.3.8/dist/css/bootstrap.min.css" }
        Stylesheet { href: "https://cdnjs.cloudflare.com/ajax/libs/font-awesome/6.5.1/css/all.min.css" }
        // Stylesheet { href: asset!("/assets/sass/index.sass") }
        Script { src: "https://cdn.jsdelivr.net/npm/@popperjs/core@2.11.8/dist/umd/popper.min.js" }
        Script { src: "https://cdn.jsdelivr.net/npm/bootstrap@5.3.8/dist/js/bootstrap.min.js" }
        Script { src: "https://cdn.plot.ly/plotly-2.14.0.min.js" }
    }
}
