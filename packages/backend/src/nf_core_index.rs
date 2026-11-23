//! The public nf-core pipeline index (`https://nf-co.re/pipelines.json`), fetched by the
//! backend so the workflow editor can offer pipelines and releases to pick from.
//!
//! The index is kept in memory and in a file below the storage root. A copy younger than the
//! TTL is served without a request. If a refresh fails (offline, rate limited, server error)
//! the last copy is served and marked stale, so the editor keeps working offline.

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use macworp_core::responses::{NfCorePipeline, NfCoreRelease};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;

/// Address of the public index.
pub const DEFAULT_INDEX_URL: &str = "https://nf-co.re/pipelines.json";

/// File name of the persisted copy.
const CACHE_FILE_NAME: &str = "nf_core_index.json";

/// Largest index accepted (the real one is a few MB).
const MAX_INDEX_BYTES: u64 = 64 * 1024 * 1024;

/// Seconds to wait after a rate limit answer without a `Retry-After` header.
const DEFAULT_BACKOFF_SECONDS: u64 = 300;

#[derive(Debug, Error)]
pub enum NfCoreIndexError {
    /// No copy of the index exists and fetching it failed
    #[error("The nf-core index is not available: {0}")]
    Unavailable(String),
}

/// A pipeline with its releases, reduced to what the editor needs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexedPipeline {
    pub name: String,
    pub description: String,
    pub archived: bool,
    /// Newest first, as the upstream index lists them
    pub releases: Vec<NfCoreRelease>,
}

impl IndexedPipeline {
    pub fn summary(&self) -> NfCorePipeline {
        NfCorePipeline {
            name: self.name.clone(),
            description: self.description.clone(),
            archived: self.archived,
        }
    }
}

/// The index at one point in time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    /// Unix seconds
    pub fetched_at: u64,
    pub pipelines: Vec<IndexedPipeline>,
}

impl Snapshot {
    /// Finds a pipeline; a leading `nf-core/` in `name` is ignored.
    ///
    /// # Arguments
    /// * `name` - Pipeline name, with or without the `nf-core/` prefix
    pub fn pipeline(&self, name: &str) -> Option<&IndexedPipeline> {
        let name = name.strip_prefix("nf-core/").unwrap_or(name);
        self.pipelines.iter().find(|pipeline| pipeline.name == name)
    }
}

/// A snapshot and whether it is older than a refresh attempt that failed.
#[derive(Clone, Debug)]
pub struct Served {
    pub snapshot: Arc<Snapshot>,
    pub stale: bool,
}

#[derive(Default)]
struct Inner {
    loaded: Option<Arc<Snapshot>>,
    /// No request before this time (rate limit answer)
    backoff_until: Option<SystemTime>,
}

/// Cached access to the nf-core pipeline index.
pub struct NfCoreIndex {
    url: String,
    ttl: Duration,
    cache_file: PathBuf,
    client: reqwest::Client,
    /// Held during a refresh, so concurrent requests cause one upstream request
    inner: Mutex<Inner>,
}

#[derive(Deserialize)]
struct RawIndex {
    remote_workflows: Vec<RawPipeline>,
}

#[derive(Deserialize)]
struct RawPipeline {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    archived: bool,
    #[serde(default)]
    releases: Vec<RawRelease>,
}

#[derive(Deserialize)]
struct RawRelease {
    tag_name: String,
    #[serde(default)]
    published_at: String,
    #[serde(default)]
    has_schema: bool,
}

fn unix_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Reduces the upstream document to a snapshot.
///
/// # Arguments
/// * `body` - Upstream JSON
/// * `fetched_at` - Unix seconds to record
fn parse_index(body: &[u8], fetched_at: u64) -> Result<Snapshot, String> {
    let raw: RawIndex = serde_json::from_slice(body).map_err(|e| e.to_string())?;
    let mut pipelines: Vec<IndexedPipeline> = raw
        .remote_workflows
        .into_iter()
        .map(|pipeline| IndexedPipeline {
            name: pipeline.name,
            description: pipeline.description.unwrap_or_default(),
            archived: pipeline.archived,
            releases: pipeline
                .releases
                .into_iter()
                .map(|release| NfCoreRelease {
                    tag: release.tag_name,
                    published_at: release.published_at,
                    has_schema: release.has_schema,
                })
                .collect(),
        })
        .collect();
    pipelines.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(Snapshot {
        fetched_at,
        pipelines,
    })
}

impl NfCoreIndex {
    /// Creates the index access, nothing is fetched yet.
    ///
    /// # Arguments
    /// * `url` - Address of `pipelines.json`
    /// * `ttl` - How long a fetched index is served without a refresh
    /// * `cache_directory` - Directory for the persisted copy (created when saving)
    pub fn new(url: String, ttl: Duration, cache_directory: PathBuf) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent("macworp-backend")
            .build()
            .unwrap_or_default();
        Self {
            url,
            ttl,
            cache_file: cache_directory.join(CACHE_FILE_NAME),
            client,
            inner: Mutex::default(),
        }
    }

    /// Returns the index: from memory or file while fresh, otherwise freshly fetched; the last
    /// copy (marked stale) if fetching fails.
    pub async fn get(&self) -> Result<Served, NfCoreIndexError> {
        let mut inner = self.inner.lock().await;
        let now = SystemTime::now();
        if inner.loaded.is_none() {
            inner.loaded = self.read_file().await.map(Arc::new);
        }
        if let Some(snapshot) = &inner.loaded {
            let age = unix_seconds(now).saturating_sub(snapshot.fetched_at);
            if age < self.ttl.as_secs() {
                return Ok(Served {
                    snapshot: snapshot.clone(),
                    stale: false,
                });
            }
        }
        let failure = if inner.backoff_until.is_some_and(|until| until > now) {
            "rate limited, waiting before the next request".to_string()
        } else {
            match self.fetch(now).await {
                Ok(snapshot) => {
                    let snapshot = Arc::new(snapshot);
                    self.write_file(&snapshot).await;
                    inner.loaded = Some(snapshot.clone());
                    inner.backoff_until = None;
                    return Ok(Served {
                        snapshot,
                        stale: false,
                    });
                }
                Err((message, backoff)) => {
                    if let Some(backoff) = backoff {
                        inner.backoff_until = Some(now + backoff);
                    }
                    message
                }
            }
        };
        match &inner.loaded {
            Some(snapshot) => {
                tracing::warn!("nf-core index refresh failed, serving the cached copy: {failure}");
                Ok(Served {
                    snapshot: snapshot.clone(),
                    stale: true,
                })
            }
            None => Err(NfCoreIndexError::Unavailable(failure)),
        }
    }

    /// Fetches the upstream index. The error carries the message and, for rate limit answers,
    /// the time to wait.
    async fn fetch(&self, now: SystemTime) -> Result<Snapshot, (String, Option<Duration>)> {
        let response = self
            .client
            .get(&self.url)
            .send()
            .await
            .map_err(|e| (e.to_string(), None))?;
        let status = response.status();
        if !status.is_success() {
            let backoff = (status == reqwest::StatusCode::TOO_MANY_REQUESTS
                || status == reqwest::StatusCode::FORBIDDEN)
                .then(|| {
                    let seconds = response
                        .headers()
                        .get(reqwest::header::RETRY_AFTER)
                        .and_then(|value| value.to_str().ok())
                        .and_then(|value| value.parse::<u64>().ok())
                        .unwrap_or(DEFAULT_BACKOFF_SECONDS);
                    Duration::from_secs(seconds)
                });
            return Err((format!("HTTP {status}"), backoff));
        }
        if response
            .content_length()
            .is_some_and(|n| n > MAX_INDEX_BYTES)
        {
            return Err(("index too large".to_string(), None));
        }
        let body = response.bytes().await.map_err(|e| (e.to_string(), None))?;
        if body.len() as u64 > MAX_INDEX_BYTES {
            return Err(("index too large".to_string(), None));
        }
        parse_index(&body, unix_seconds(now)).map_err(|e| (format!("invalid index: {e}"), None))
    }

    async fn read_file(&self) -> Option<Snapshot> {
        let content = tokio::fs::read(&self.cache_file).await.ok()?;
        serde_json::from_slice(&content).ok()
    }

    /// Persists a snapshot atomically; failures are logged only, the copy stays in memory.
    async fn write_file(&self, snapshot: &Snapshot) {
        let result = async {
            let content = serde_json::to_vec(snapshot).map_err(std::io::Error::other)?;
            if let Some(parent) = self.cache_file.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            let temporary = self.cache_file.with_extension("json.tmp");
            tokio::fs::write(&temporary, content).await?;
            tokio::fs::rename(&temporary, &self.cache_file).await
        }
        .await;
        if let Err(e) = result {
            tracing::warn!("Unable to save the nf-core index copy: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::{Router, http::StatusCode, response::IntoResponse, routing::get};

    use super::*;

    const BODY: &str = r#"{"remote_workflows":[
        {"name":"rnaseq","description":"RNA","archived":false,"stargazers_count":1,
         "releases":[{"tag_name":"3.2","published_at":"2026-01-01T00:00:00Z","has_schema":true,"doc_files":["a"]},
                     {"tag_name":"3.1","published_at":"2025-01-01T00:00:00Z","has_schema":false}]},
        {"name":"demo","description":null,"archived":true,"releases":[]}]}"#;

    /// Upstream with a scripted status, counting requests.
    struct Upstream {
        url: String,
        hits: Arc<AtomicUsize>,
        status: Arc<AtomicUsize>,
    }

    async fn upstream(body: &'static str) -> Upstream {
        let hits = Arc::new(AtomicUsize::new(0));
        let status = Arc::new(AtomicUsize::new(200));
        let (h, s) = (hits.clone(), status.clone());
        let app = Router::new().route(
            "/pipelines.json",
            get(move || {
                let (h, s) = (h.clone(), s.clone());
                async move {
                    h.fetch_add(1, Ordering::SeqCst);
                    let code = s.load(Ordering::SeqCst) as u16;
                    (StatusCode::from_u16(code).unwrap(), body).into_response()
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/pipelines.json", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Upstream { url, hits, status }
    }

    fn index(upstream: &Upstream, ttl: u64, directory: &std::path::Path) -> NfCoreIndex {
        NfCoreIndex::new(
            upstream.url.clone(),
            Duration::from_secs(ttl),
            directory.to_path_buf(),
        )
    }

    #[test]
    fn parse_keeps_only_needed_fields_sorted() {
        let snapshot = parse_index(BODY.as_bytes(), 7).unwrap();
        assert_eq!(snapshot.fetched_at, 7);
        let names: Vec<_> = snapshot.pipelines.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["demo", "rnaseq"]);
        assert_eq!(snapshot.pipelines[0].description, "");
        assert!(snapshot.pipelines[0].archived);
        let releases = &snapshot.pipelines[1].releases;
        assert_eq!(releases[0].tag, "3.2");
        assert!(releases[0].has_schema);
        assert_eq!(releases[1].tag, "3.1");
    }

    #[test]
    fn pipeline_lookup_ignores_prefix() {
        let snapshot = parse_index(BODY.as_bytes(), 0).unwrap();
        assert!(snapshot.pipeline("nf-core/rnaseq").is_some());
        assert!(snapshot.pipeline("rnaseq").is_some());
        assert!(snapshot.pipeline("other").is_none());
    }

    #[tokio::test]
    async fn fresh_copy_is_served_without_request() {
        let upstream = upstream(BODY).await;
        let directory = tempfile::tempdir().unwrap();
        let index = index(&upstream, 3600, directory.path());
        assert!(!index.get().await.unwrap().stale);
        assert!(!index.get().await.unwrap().stale);
        assert_eq!(upstream.hits.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn expired_copy_is_fetched_again() {
        let upstream = upstream(BODY).await;
        let directory = tempfile::tempdir().unwrap();
        let index = index(&upstream, 0, directory.path());
        index.get().await.unwrap();
        index.get().await.unwrap();
        assert_eq!(upstream.hits.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn failing_upstream_serves_stale_memory_copy() {
        let upstream = upstream(BODY).await;
        let directory = tempfile::tempdir().unwrap();
        let index = index(&upstream, 0, directory.path());
        index.get().await.unwrap();
        upstream.status.store(500, Ordering::SeqCst);
        let served = index.get().await.unwrap();
        assert!(served.stale);
        assert_eq!(served.snapshot.pipelines.len(), 2);
    }

    #[tokio::test]
    async fn new_instance_uses_the_file_copy() {
        let upstream = upstream(BODY).await;
        let directory = tempfile::tempdir().unwrap();
        index(&upstream, 3600, directory.path())
            .get()
            .await
            .unwrap();
        // restart within the TTL: no request
        let fresh = index(&upstream, 3600, directory.path());
        assert!(!fresh.get().await.unwrap().stale);
        assert_eq!(upstream.hits.load(Ordering::SeqCst), 1);
        // restart after the TTL with upstream down: stale from file
        upstream.status.store(500, Ordering::SeqCst);
        let expired = index(&upstream, 0, directory.path());
        assert!(expired.get().await.unwrap().stale);
    }

    #[tokio::test]
    async fn no_copy_and_failing_upstream_is_unavailable() {
        let upstream = upstream(BODY).await;
        upstream.status.store(500, Ordering::SeqCst);
        let directory = tempfile::tempdir().unwrap();
        let index = index(&upstream, 3600, directory.path());
        assert!(matches!(
            index.get().await,
            Err(NfCoreIndexError::Unavailable(_))
        ));
    }

    #[tokio::test]
    async fn garbage_is_not_cached() {
        let upstream = upstream("not json").await;
        let directory = tempfile::tempdir().unwrap();
        let index = index(&upstream, 3600, directory.path());
        assert!(index.get().await.is_err());
        assert!(!directory.path().join(CACHE_FILE_NAME).exists());
    }

    #[tokio::test]
    async fn rate_limit_backs_off() {
        let upstream = upstream(BODY).await;
        let directory = tempfile::tempdir().unwrap();
        let index = index(&upstream, 0, directory.path());
        index.get().await.unwrap();
        upstream.status.store(429, Ordering::SeqCst);
        assert!(index.get().await.unwrap().stale);
        let hits = upstream.hits.load(Ordering::SeqCst);
        // backing off: no further request even though the copy is expired
        assert!(index.get().await.unwrap().stale);
        assert_eq!(upstream.hits.load(Ordering::SeqCst), hits);
    }
}
