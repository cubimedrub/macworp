use dioxus::prelude::*;

use macworp_ui_core::{configuration::Configuration, routes::Route, FAVICON, MAIN_CSS};

#[tokio::main]
async fn main() {
    // The `launch` function is the main entry point for a dioxus app. It takes a component and renders it with the platform feature
    // you have enabled
    tracing_subscriber::fmt::init();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();

    let base_url = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port(),);

    let (backend_shutdown_signal_sender, backend_shutdown_signal_receiver) =
        tokio::sync::oneshot::channel::<()>();
    let backend_shutdown_signal = async move {
        backend_shutdown_signal_receiver.await.ok();
    };

    let backend = tokio::spawn(async {
        macworp_backend::start(listener, Box::pin(backend_shutdown_signal)).await
    });

    let config = Configuration::new(base_url);

    dioxus::LaunchBuilder::new()
        .with_context(config)
        .launch(App);

    backend_shutdown_signal_sender.send(()).unwrap();

    backend.await.unwrap();
}

#[component]
fn App() -> Element {
    let config = use_resource(async || use_context::<Configuration>());
    use_context_provider(|| config);

    rsx! {
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }

        Router::<Route> {}
    }
}
