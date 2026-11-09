use std::{
    collections::HashMap,
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use macworp_core::{
    entities::LogStream,
    requests::{
        WorkerAnnounceRequest, WorkerClaimRequest, WorkerFinishRequest, WorkerLogsRequest,
        WorkerOutcome, WorkerProgressRequest,
    },
    responses::{
        StorageProbe, WorkerAnnounceResponse, WorkerClaimResponse, WorkerHeartbeatResponse,
    },
    routing::{IsRouting, WorkerRouting},
    workflow_configuration::engine::Engine,
};
use reqwest::{Client, RequestBuilder, StatusCode};

use crate::{
    container_runtime::BoxFuture,
    executors::{Job, Outcome},
    job_source::{Error, JobSource},
    progress::ProgressUpdate,
};

/// Time after which a single request to the backend is given up
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// [`JobSource`] of a remote worker: claims runs from the backend over HTTP and reports back.
///
/// `claim` authenticates with the worker credential (Basic); the backend answers with a per-run
/// token, which this source keeps in memory and sends with every further call of that run. One
/// instance can serve several job slots, tokens are kept per run.
pub struct HttpJobSource {
    client: Client,
    /// Backend base URL without trailing slash
    base_url: String,
    /// Worker credential
    username: String,
    password: String,
    /// Name reported to the backend, identifies this worker process in leases
    worker_name: String,
    /// Engines this worker can run, sent with every claim so only matching runs are leased
    engines: Vec<Engine>,
    /// The project storage matches the backend's (see [`HttpJobSource::announce`]); while it
    /// does not, nothing is claimed and no engine is announced
    storage_ok: AtomicBool,
    /// Tokens of the runs claimed through this source
    tokens: Mutex<HashMap<i64, String>>,
}

impl HttpJobSource {
    /// Creates the source.
    ///
    /// # Arguments
    /// * `base_url` - backend root, e.g. `https://macworp.example.org`
    /// * `username` - worker credential
    /// * `password` - worker credential
    /// * `worker_name` - name identifying this worker process in leases
    /// * `engines` - engines this worker can run; the backend leases it only runs of these
    pub fn new(
        base_url: &str,
        username: String,
        password: String,
        worker_name: String,
        engines: Vec<Engine>,
    ) -> Result<Self, Error> {
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| Error(format!("Cannot create HTTP client: {e}")))?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            username,
            password,
            worker_name,
            engines,
            storage_ok: AtomicBool::new(true),
            tokens: Mutex::new(HashMap::new()),
        })
    }

    /// Full URL of a worker endpoint, `{run_id}` in `path` is replaced
    fn url(&self, path: &str, run_id: Option<i64>) -> String {
        let path = match run_id {
            Some(id) => path.replace("{run_id}", &id.to_string()),
            None => path.to_string(),
        };
        format!(
            "{}{}{}",
            self.base_url,
            WorkerRouting::controller_path(),
            path
        )
    }

    /// The token of a claimed run
    fn token(&self, run_id: i64) -> Result<String, Error> {
        self.tokens
            .lock()
            .expect("token map poisoned")
            .get(&run_id)
            .cloned()
            .ok_or_else(|| Error(format!("No token for run {run_id}")))
    }

    /// Error for calls that cannot be skipped when the lease is gone
    fn lease_lost(run_id: i64) -> Error {
        Error(format!("Run {run_id} is no longer leased to this worker"))
    }

    /// Sends an authenticated per-run request. `Ok(None)` means the backend rejected the token
    /// (401): the lease expired and the run was requeued, failed or finished, it is not ours anymore.
    async fn send_for_run(
        &self,
        run_id: i64,
        request: impl FnOnce(&Client) -> RequestBuilder,
    ) -> Result<Option<reqwest::Response>, Error> {
        let token = self.token(run_id)?;
        let response = request(&self.client)
            .header(reqwest::header::AUTHORIZATION, format!("Run {token}"))
            .send()
            .await
            .map_err(|e| Error(format!("Backend unreachable: {e}")))?;
        match response.status() {
            status if status.is_success() => Ok(Some(response)),
            StatusCode::UNAUTHORIZED => Ok(None),
            status => Err(Error(format!("Backend answered {status}"))),
        }
    }
}

impl HttpJobSource {
    /// Tells the backend which runtime and engines this worker offers and that it is alive, and
    /// checks the shared project storage with the marker file the backend names in its answer.
    /// While the marker is unreadable or differs, the worker announces no engines and
    /// [`JobSource::claim`] takes nothing, so runs wait for a worker that sees the storage
    /// instead of running on an empty or wrong directory. Returns the seconds the announcement
    /// stays valid.
    ///
    /// # Arguments
    /// * `runtime` - container runtime binary name, e.g. `podman`
    /// * `runtime_version` - its version if known
    /// * `engines` - engines this worker can run
    pub async fn announce(
        &self,
        runtime: &str,
        runtime_version: Option<&str>,
        engines: &[Engine],
    ) -> Result<u32, Error> {
        let announced_engines: &[Engine] = if self.storage_ok.load(Ordering::Relaxed) {
            engines
        } else {
            &[]
        };
        let response = self
            .client
            .post(self.url(WorkerRouting::ANNOUNCE_PATH, None))
            .basic_auth(&self.username, Some(&self.password))
            .json(&WorkerAnnounceRequest {
                worker_name: self.worker_name.clone(),
                runtime: runtime.to_string(),
                runtime_version: runtime_version.map(str::to_string),
                engines: announced_engines.to_vec(),
            })
            .send()
            .await
            .map_err(|e| Error(format!("Backend unreachable: {e}")))?;
        if !response.status().is_success() {
            return Err(Error(format!("Backend answered {}", response.status())));
        }
        let announced: WorkerAnnounceResponse = response
            .json()
            .await
            .map_err(|e| Error(format!("Invalid announce response: {e}")))?;
        let problem = match &announced.storage {
            Some(probe) => Self::check_storage(probe).await.err(),
            None => None,
        };
        let ok = problem.is_none();
        if self.storage_ok.swap(ok, Ordering::Relaxed) != ok {
            match &problem {
                Some(problem) => tracing::error!("{problem}; not taking runs until it is fixed"),
                None => tracing::info!("Shared project storage is visible again"),
            }
        }
        Ok(announced.ttl_seconds)
    }

    /// Whether this machine sees the backend's project storage: the marker file exists at the
    /// same path with the same content. The error says what is wrong.
    ///
    /// # Arguments
    /// * `probe` - marker file as named by the backend
    async fn check_storage(probe: &StorageProbe) -> Result<(), String> {
        match tokio::fs::read_to_string(&probe.path).await {
            Ok(content) if content.trim() == probe.content => Ok(()),
            Ok(_) => Err(format!(
                "Shared storage marker {} has a different content than on the backend: the worker sees another filesystem",
                probe.path.display()
            )),
            Err(e) => Err(format!(
                "Shared storage marker {} is not readable ({e}): mount the backend's projects directory at the same path",
                probe.path.display()
            )),
        }
    }
}

impl JobSource for HttpJobSource {
    fn claim(&self) -> BoxFuture<'_, Result<Option<Job>, Error>> {
        Box::pin(async move {
            if !self.storage_ok.load(Ordering::Relaxed) {
                return Ok(None);
            }
            let response = self
                .client
                .post(self.url(WorkerRouting::CLAIM_PATH, None))
                .basic_auth(&self.username, Some(&self.password))
                .json(&WorkerClaimRequest {
                    worker_name: self.worker_name.clone(),
                    engines: self.engines.clone(),
                })
                .send()
                .await
                .map_err(|e| Error(format!("Backend unreachable: {e}")))?;
            match response.status() {
                StatusCode::NO_CONTENT => Ok(None),
                status if status.is_success() => {
                    let claimed: WorkerClaimResponse = response
                        .json()
                        .await
                        .map_err(|e| Error(format!("Invalid claim response: {e}")))?;
                    self.tokens
                        .lock()
                        .expect("token map poisoned")
                        .insert(claimed.run_id, claimed.token);
                    Ok(Some(Job {
                        run_id: claimed.run_id,
                        project_dir: claimed.project_dir,
                        configuration: claimed.configuration,
                        progress: None,
                    }))
                }
                status => Err(Error(format!("Claim rejected by the backend: {status}"))),
            }
        })
    }

    fn append_logs(
        &self,
        run_id: i64,
        stream: LogStream,
        lines: Vec<String>,
    ) -> BoxFuture<'_, Result<(), Error>> {
        Box::pin(async move {
            let body = WorkerLogsRequest { stream, lines };
            self.send_for_run(run_id, |client| {
                client
                    .post(self.url(WorkerRouting::LOGS_PATH, Some(run_id)))
                    .json(&body)
            })
            .await?
            .map(|_| ())
            .ok_or_else(|| Self::lease_lost(run_id))
        })
    }

    fn report_progress(
        &self,
        run_id: i64,
        update: ProgressUpdate,
    ) -> BoxFuture<'_, Result<(), Error>> {
        Box::pin(async move {
            let body = WorkerProgressRequest {
                total: update.total,
                done: update.done,
                message: update.message,
                error: update.error,
            };
            self.send_for_run(run_id, |client| {
                client
                    .post(self.url(WorkerRouting::PROGRESS_PATH, Some(run_id)))
                    .json(&body)
            })
            .await?
            .map(|_| ())
            .ok_or_else(|| Self::lease_lost(run_id))
        })
    }

    /// Doubles as the heartbeat: the worker polls this while a run is active, each call extends
    /// the lease. A lost lease (401) is answered with `true`, the engine must stop because
    /// the backend already handed the run to someone else or gave up on it.
    fn cancel_requested(&self, run_id: i64) -> BoxFuture<'_, Result<bool, Error>> {
        Box::pin(async move {
            let Some(response) = self
                .send_for_run(run_id, |client| {
                    client.post(self.url(WorkerRouting::HEARTBEAT_PATH, Some(run_id)))
                })
                .await?
            else {
                tracing::warn!("Lease of run {run_id} lost, stopping it");
                return Ok(true);
            };
            let heartbeat: WorkerHeartbeatResponse = response
                .json()
                .await
                .map_err(|e| Error(format!("Invalid heartbeat response: {e}")))?;
            Ok(heartbeat.cancel_requested)
        })
    }

    fn finish(&self, run_id: i64, outcome: Outcome) -> BoxFuture<'_, Result<(), Error>> {
        Box::pin(async move {
            let body = WorkerFinishRequest {
                outcome: match outcome {
                    Outcome::Succeeded => WorkerOutcome::Succeeded,
                    Outcome::Failed {
                        exit_code,
                        error,
                        reason,
                    } => WorkerOutcome::Failed {
                        exit_code,
                        error,
                        reason,
                    },
                    Outcome::Cancelled => WorkerOutcome::Cancelled,
                },
            };
            // the token is dropped whatever happens: after finish the run is not ours anymore
            let result = self
                .send_for_run(run_id, |client| {
                    client
                        .post(self.url(WorkerRouting::FINISH_PATH, Some(run_id)))
                        .json(&body)
                })
                .await;
            self.tokens
                .lock()
                .expect("token map poisoned")
                .remove(&run_id);
            result.map(|_| ())
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::{
        Json, Router,
        extract::State,
        http::{HeaderMap, StatusCode, header::AUTHORIZATION},
        routing::post,
    };
    use macworp_core::{requests::WorkerOutcome, workflow_configuration::WorkflowConfiguration};

    use super::*;

    /// Engine lists the fake backend received with announcements
    type Announced = Arc<Mutex<Vec<Vec<Engine>>>>;

    /// What the fake backend saw: (path, Authorization header)
    type Seen = Arc<Mutex<Vec<(String, String)>>>;

    /// Starts a fake backend. `claim` hands out run 7 with token `tok`; per-run endpoints answer
    /// 204 (heartbeat: `cancel_requested: false`) for the token `tok` and 401 for anything else.
    async fn backend() -> (String, Seen) {
        let seen: Seen = Arc::default();
        async fn record(seen: &Seen, path: &str, headers: &HeaderMap) -> String {
            let header = headers
                .get(AUTHORIZATION)
                .map(|value| value.to_str().unwrap().to_string())
                .unwrap_or_default();
            seen.lock()
                .unwrap()
                .push((path.to_string(), header.clone()));
            header
        }
        async fn claim(State(seen): State<Seen>, headers: HeaderMap) -> axum::response::Response {
            use axum::response::IntoResponse;
            record(&seen, "claim", &headers).await;
            Json(WorkerClaimResponse {
                run_id: 7,
                project_dir: "/data/p".into(),
                configuration: WorkflowConfiguration::default(),
                token: "tok".into(),
                lease_seconds: 60,
            })
            .into_response()
        }
        async fn heartbeat(
            State(seen): State<Seen>,
            headers: HeaderMap,
        ) -> axum::response::Response {
            use axum::response::IntoResponse;
            if record(&seen, "heartbeat", &headers).await != "Run tok" {
                return StatusCode::UNAUTHORIZED.into_response();
            }
            Json(WorkerHeartbeatResponse {
                cancel_requested: false,
            })
            .into_response()
        }
        async fn finish(
            State(seen): State<Seen>,
            headers: HeaderMap,
            Json(request): Json<WorkerFinishRequest>,
        ) -> StatusCode {
            if record(&seen, "finish", &headers).await != "Run tok" {
                return StatusCode::UNAUTHORIZED;
            }
            assert_eq!(request.outcome, WorkerOutcome::Succeeded);
            StatusCode::NO_CONTENT
        }
        let router = Router::new()
            .route("/worker/claim", post(claim))
            .route("/worker/heartbeat/7", post(heartbeat))
            .route("/worker/finish/7", post(finish))
            .with_state(seen.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        (url, seen)
    }

    /// Claim uses Basic, later calls the per-run token; the token is dropped after finish and
    /// then a heartbeat fails (no token) instead of leaking the credential
    #[tokio::test]
    async fn claim_uses_basic_then_run_token() {
        let (url, seen) = backend().await;
        let source = HttpJobSource::new(
            &format!("{url}/"),
            "w".into(),
            "pw".into(),
            "n".into(),
            vec![Engine::Nextflow],
        )
        .unwrap();
        let job = source.claim().await.unwrap().unwrap();
        assert_eq!(job.run_id, 7);
        assert_eq!(job.project_dir, std::path::PathBuf::from("/data/p"));
        assert!(!source.cancel_requested(7).await.unwrap());
        source.finish(7, Outcome::Succeeded).await.unwrap();
        assert!(source.cancel_requested(7).await.is_err());

        let seen = seen.lock().unwrap();
        assert!(seen[0].1.starts_with("Basic "));
        assert_eq!(seen[1], ("heartbeat".into(), "Run tok".into()));
        assert_eq!(seen[2], ("finish".into(), "Run tok".into()));
        assert_eq!(seen.len(), 3);
    }

    /// A rejected run token (lease lost) stops the run through the cancel poll and does not make
    /// `finish` fail; log calls fail so the worker notices
    #[tokio::test]
    async fn lost_lease_cancels_and_finish_is_dropped() {
        let (url, _) = backend().await;
        let source = HttpJobSource::new(
            &url,
            "w".into(),
            "pw".into(),
            "n".into(),
            vec![Engine::Nextflow],
        )
        .unwrap();
        source.claim().await.unwrap().unwrap();
        // the fake only accepts `tok`; swap the stored token for a stale one
        source.tokens.lock().unwrap().insert(7, "stale".into());
        assert!(source.cancel_requested(7).await.unwrap());
        source.finish(7, Outcome::Succeeded).await.unwrap();
    }

    /// Announce sends Basic credentials and the engines, and returns the backend's ttl
    #[tokio::test]
    async fn announce_sends_engines_and_returns_ttl() {
        async fn announce(
            headers: HeaderMap,
            Json(request): Json<WorkerAnnounceRequest>,
        ) -> Json<WorkerAnnounceResponse> {
            assert!(
                headers[AUTHORIZATION]
                    .to_str()
                    .unwrap()
                    .starts_with("Basic ")
            );
            assert_eq!(request.worker_name, "n");
            assert_eq!(request.runtime, "podman");
            assert_eq!(request.engines, vec![Engine::Nextflow]);
            Json(WorkerAnnounceResponse {
                ttl_seconds: 45,
                storage: None,
            })
        }
        let router = Router::new().route("/worker/announce", post(announce));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });

        let source = HttpJobSource::new(
            &url,
            "w".into(),
            "pw".into(),
            "n".into(),
            vec![Engine::Nextflow],
        )
        .unwrap();
        let ttl = source
            .announce("podman", Some("5.0"), &[Engine::Nextflow])
            .await
            .unwrap();
        assert_eq!(ttl, 45);
    }

    /// A worker that cannot read the backend's storage marker withdraws: no engines announced,
    /// nothing claimed; it recovers when the marker matches again
    #[tokio::test]
    async fn missing_storage_marker_stops_claims_and_engines() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join(".macworp-storage-id");
        let announced: Announced = Arc::default();
        let state = (announced.clone(), marker.clone());
        async fn announce(
            State((announced, marker)): State<(Announced, std::path::PathBuf)>,
            Json(request): Json<WorkerAnnounceRequest>,
        ) -> Json<WorkerAnnounceResponse> {
            announced.lock().unwrap().push(request.engines);
            Json(WorkerAnnounceResponse {
                ttl_seconds: 45,
                storage: Some(StorageProbe {
                    path: marker,
                    content: "abc".into(),
                }),
            })
        }
        async fn claim() -> StatusCode {
            StatusCode::NO_CONTENT
        }
        let router = Router::new()
            .route("/worker/announce", post(announce))
            .route("/worker/claim", post(claim))
            .with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let source = HttpJobSource::new(
            &url,
            "w".into(),
            "pw".into(),
            "n".into(),
            vec![Engine::Nextflow],
        )
        .unwrap();
        let engines = [Engine::Nextflow];

        // Marker missing: announced (the first time optimistically), then withdrawn
        source.announce("podman", None, &engines).await.unwrap();
        assert!(!source.storage_ok.load(Ordering::Relaxed));
        source.announce("podman", None, &engines).await.unwrap();
        assert_eq!(
            announced.lock().unwrap().last().unwrap(),
            &Vec::<Engine>::new()
        );
        assert!(source.claim().await.unwrap().is_none());

        // Wrong content: still withdrawn
        std::fs::write(&marker, "other").unwrap();
        source.announce("podman", None, &engines).await.unwrap();
        assert!(!source.storage_ok.load(Ordering::Relaxed));

        // Matching marker: back
        std::fs::write(&marker, "abc\n").unwrap();
        source.announce("podman", None, &engines).await.unwrap();
        assert!(source.storage_ok.load(Ordering::Relaxed));
        source.announce("podman", None, &engines).await.unwrap();
        assert_eq!(announced.lock().unwrap().last().unwrap(), &engines.to_vec());
    }
}
