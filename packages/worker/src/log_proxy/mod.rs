use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use axum::{
    Form, Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post, put},
};
use macworp_core::workflow_configuration::engine::Engine;
use rand::{RngExt, distr::Alphanumeric};
use serde::Deserialize;
use serde_json::json;
use tokio::{net::TcpListener, sync::mpsc, task::JoinHandle};

use crate::container_runtime::RuntimeKind;
use crate::progress::{ProgressTracker, ProgressUpdate};

/// Where an engine reports its progress and how its container gets there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProgressEndpoint {
    /// `http://<host>:<port>/<token>`, the engine appends its own path (`/nextflow`, `/snakemake`)
    pub base_url: String,
    /// Extra `<runtime> run` arguments the container needs to reach the host
    pub run_args: Vec<String>,
}

/// How the engine containers reach the worker's receiver.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostAccess {
    /// Name of the host as seen from a container
    pub host: String,
    /// `<runtime> run` arguments that make the name resolve
    pub run_args: Vec<String>,
}

impl HostAccess {
    /// How containers of the runtime reach the host, `None` if the runtime runs no engines that report progress
    pub fn for_runtime(kind: RuntimeKind) -> Option<Self> {
        match kind {
            RuntimeKind::Podman => Some(Self::podman()),
            RuntimeKind::Docker => Some(Self::docker()),
            #[cfg(windows)]
            RuntimeKind::Wslc => None,
        }
    }

    /// Podman provides `host.containers.internal` itself, also in its VM on macOS/Windows
    pub fn podman() -> Self {
        Self {
            host: "host.containers.internal".into(),
            run_args: Vec::new(),
        }
    }

    /// Docker Desktop provides `host.docker.internal`, on Linux it has to be mapped to the gateway
    pub fn docker() -> Self {
        Self {
            host: "host.docker.internal".into(),
            run_args: vec![
                "--add-host".into(),
                "host.docker.internal:host-gateway".into(),
            ],
        }
    }
}

struct Registration {
    run_id: i64,
    tracker: ProgressTracker,
    updates: mpsc::UnboundedSender<ProgressUpdate>,
}

type Registry = Arc<Mutex<HashMap<String, Registration>>>;

/// A run registered at the receiver. Dropping it unregisters the run, the engine's token stops working.
pub struct RegisteredRun {
    pub endpoint: ProgressEndpoint,
    updates: Option<mpsc::UnboundedReceiver<ProgressUpdate>>,
    token: String,
    registry: Registry,
}

impl RegisteredRun {
    /// The progress updates of the run, once. The channel closes when `self` is dropped.
    pub fn take_updates(&mut self) -> Option<mpsc::UnboundedReceiver<ProgressUpdate>> {
        self.updates.take()
    }
}

impl Drop for RegisteredRun {
    fn drop(&mut self) {
        if let Ok(mut registry) = self.registry.lock() {
            registry.remove(&self.token);
        }
    }
}

/// HTTP server the engines report their progress to: Nextflow with `-with-weblog`, Snakemake with
/// `--logger panoptes` (it mimics the Panoptes API the logger plugin expects). The events are turned into
/// [`ProgressUpdate`]s per run.
///
/// The engine runs in a container, so the server listens on all interfaces. Every run gets an
/// unguessable token as first path segment, requests with another token are rejected.
pub struct ProgressReceiver {
    port: u16,
    access: HostAccess,
    registry: Registry,
    server: JoinHandle<()>,
}

impl Drop for ProgressReceiver {
    fn drop(&mut self) {
        self.server.abort();
    }
}

impl ProgressReceiver {
    /// Starts listening on a free port.
    pub async fn start(access: HostAccess) -> std::io::Result<Self> {
        Self::start_on(access, ([0, 0, 0, 0], 0).into()).await
    }

    async fn start_on(access: HostAccess, addr: SocketAddr) -> std::io::Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        let port = listener.local_addr()?.port();
        let registry = Registry::default();
        let app = Self::router(registry.clone());
        let server = tokio::spawn(async move {
            if let Err(e) = axum::serve(listener, app).await {
                tracing::error!("Progress receiver stopped: {e}");
            }
        });
        Ok(Self {
            port,
            access,
            registry,
            server,
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// Registers a run. Only engines that report progress get an endpoint, `None` for the others.
    ///
    /// # Arguments
    /// * `run_id` - run the progress belongs to
    /// * `engine` - engine of the run, decides which events are understood
    /// * `access` - how the run's container reaches this host, the receiver's own access if `None`
    pub fn register(
        &self,
        run_id: i64,
        engine: &Engine,
        access: Option<&HostAccess>,
    ) -> Option<RegisteredRun> {
        let access = access.unwrap_or(&self.access);
        if !matches!(engine, Engine::Nextflow | Engine::Snakemake) {
            return None;
        }
        let token: String = rand::rng()
            .sample_iter(Alphanumeric)
            .take(32)
            .map(char::from)
            .collect();
        let (updates_tx, updates) = mpsc::unbounded_channel();
        self.registry.lock().ok()?.insert(
            token.clone(),
            Registration {
                run_id,
                tracker: ProgressTracker::new(engine.clone()),
                updates: updates_tx,
            },
        );
        Some(RegisteredRun {
            endpoint: ProgressEndpoint {
                base_url: format!("http://{}:{}/{token}", access.host, self.port),
                run_args: access.run_args.clone(),
            },
            updates: Some(updates),
            token,
            registry: self.registry.clone(),
        })
    }

    fn router(registry: Registry) -> Router {
        Router::new()
            .route("/{token}/nextflow", post(nextflow_event))
            .route(
                "/{token}/snakemake/api/service-info",
                get(|| async { Json(json!({ "status": "running" })) }),
            )
            .route("/{token}/snakemake/create_workflow", get(snakemake_create))
            .route(
                "/{token}/snakemake/update_workflow_status",
                post(snakemake_event),
            )
            .route(
                "/{token}/snakemake/api/workflow/{id}",
                put(|| async { StatusCode::OK }),
            )
            .with_state(registry)
    }
}

/// Feeds one event of a run to its tracker and forwards the update. The engine always gets a
/// success for known tokens, a broken or unknown event must never fail the workflow.
fn handle_event(registry: &Registry, token: &str, payload: &str) -> StatusCode {
    let Ok(mut registry) = registry.lock() else {
        return StatusCode::OK;
    };
    let Some(run) = registry.get_mut(token) else {
        return StatusCode::NOT_FOUND;
    };
    if let Some(update) = run.tracker.handle(payload) {
        // the receiver of the run is gone when it ended, nothing to do then
        let _ = run.updates.send(update);
    }
    StatusCode::OK
}

async fn nextflow_event(
    State(registry): State<Registry>,
    Path(token): Path<String>,
    body: String,
) -> StatusCode {
    handle_event(&registry, &token, &body)
}

async fn snakemake_create(
    State(registry): State<Registry>,
    Path(token): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let registry = registry.lock().map_err(|_| StatusCode::OK)?;
    let run = registry.get(&token).ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(json!({ "id": run.run_id })))
}

/// Form body of a Snakemake 8/9 status update: `msg` holds the JSON message as text.
#[derive(Deserialize)]
struct SnakemakeForm {
    msg: String,
}

/// Status update of Snakemake. Versions 8 and 9 post a form (`msg=<json>`), version 7 a JSON body
/// (`{"msg": "<json>", ...}`) without a content type, so the body decides: a form content type is
/// read as a form, anything else as JSON first and as a form second. Unreadable bodies are
/// answered with success like every other event, they must never fail the workflow.
async fn snakemake_event(
    State(registry): State<Registry>,
    Path(token): Path<String>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> StatusCode {
    use axum::extract::FromRequest;
    const FORM: &str = "application/x-www-form-urlencoded";
    let is_form = headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with(FORM));
    let from_json = || match serde_json::from_slice::<serde_json::Value>(&body)
        .ok()?
        .get("msg")?
    {
        serde_json::Value::String(text) => Some(text.clone()),
        other => Some(other.to_string()),
    };
    let from_form = async || {
        // `Form` reads the query of a GET request, so the rebuilt request is a POST
        let request = axum::extract::Request::builder()
            .method(axum::http::Method::POST)
            .header(axum::http::header::CONTENT_TYPE, FORM)
            .body(axum::body::Body::from(body.clone()))
            .ok()?;
        Form::<SnakemakeForm>::from_request(request, &())
            .await
            .ok()
            .map(|Form(form)| form.msg)
    };
    let message = if is_form {
        from_form().await
    } else {
        match from_json() {
            Some(message) => Some(message),
            None => from_form().await,
        }
    };
    match message {
        Some(message) => handle_event(&registry, &token, &message),
        None => StatusCode::OK,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn start() -> ProgressReceiver {
        let access = HostAccess {
            host: "127.0.0.1".into(),
            run_args: vec![],
        };
        ProgressReceiver::start_on(access, ([127, 0, 0, 1], 0).into())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn nextflow_events_become_updates() {
        let receiver = start().await;
        let mut run = receiver.register(7, &Engine::Nextflow, None).unwrap();
        let mut updates = run.take_updates().unwrap();
        let client = reqwest::Client::new();
        let url = format!("{}/nextflow", run.endpoint.base_url);
        let body = r#"{"event":"process_submitted","trace":{"task_id":1,"name":"a","status":"SUBMITTED"}}"#;
        let response = client.post(&url).body(body).send().await.unwrap();
        assert!(response.status().is_success());
        let update = updates.recv().await.unwrap();
        assert_eq!((update.total, update.done), (1, 0));

        // garbage is accepted and ignored, the workflow must not fail because of it
        let response = client.post(&url).body("garbage").send().await.unwrap();
        assert!(response.status().is_success());
        assert!(updates.try_recv().is_err());
    }

    #[tokio::test]
    async fn snakemake_speaks_panoptes() {
        let receiver = start().await;
        let mut run = receiver.register(7, &Engine::Snakemake, None).unwrap();
        let mut updates = run.take_updates().unwrap();
        let client = reqwest::Client::new();
        let base = format!("{}/snakemake", run.endpoint.base_url);

        let info: serde_json::Value = client
            .get(format!("{base}/api/service-info"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(info["status"], "running");
        let created: serde_json::Value = client
            .get(format!("{base}/create_workflow"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(created["id"], 7);

        let response = client
            .post(format!("{base}/update_workflow_status"))
            .form(&[
                ("id", "7"),
                ("msg", r#"{"level":"progress","done":1,"total":3}"#),
            ])
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
        let update = updates.recv().await.unwrap();
        assert_eq!((update.total, update.done), (3, 1));

        // Snakemake 7 posts a JSON body without a content type instead of a form
        let body = serde_json::json!({
            "id": "7",
            "timestamp": "now",
            "msg": r#"{"level":"progress","done":2,"total":3}"#,
        });
        let response = client
            .post(format!("{base}/update_workflow_status"))
            .body(body.to_string())
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
        let update = updates.recv().await.unwrap();
        assert_eq!((update.total, update.done), (3, 2));
    }

    #[tokio::test]
    async fn unknown_or_dropped_token_is_rejected() {
        let receiver = start().await;
        let client = reqwest::Client::new();
        let url = format!("http://127.0.0.1:{}/nope/nextflow", receiver.port());
        assert_eq!(
            client.post(&url).body("{}").send().await.unwrap().status(),
            404
        );

        let run = receiver.register(1, &Engine::Nextflow, None).unwrap();
        let url = format!("{}/nextflow", run.endpoint.base_url);
        drop(run);
        assert_eq!(
            client.post(&url).body("{}").send().await.unwrap().status(),
            404
        );
    }

    #[tokio::test]
    async fn engines_without_progress_get_no_endpoint() {
        let receiver = start().await;
        assert!(receiver.register(1, &Engine::Container, None).is_none());
    }
}
