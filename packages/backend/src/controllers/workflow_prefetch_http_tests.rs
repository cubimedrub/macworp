use std::path::Path;

use git2::{Oid, Repository, Signature};
use http::{Method, StatusCode};
use macworp_core::workflow_configuration::{
    WorkflowConfiguration,
    source::{RemoteSource, Source},
};
use serde_json::{Value, json};

use crate::test_support::TestApp;

/// URL the tests give their workflows; the mirror rewrites it to a local repository.
const REMOTE_URL: &str = "https://local/o/r";

/// Update body for a workflow with the given configuration.
///
/// # Arguments
/// * `configuration` - Configuration to store
fn update_body(configuration: &WorkflowConfiguration) -> Value {
    json!({"name": "w", "description": "", "configuration": configuration})
}

/// Configuration following the default branch of [`REMOTE_URL`].
fn remote_configuration() -> WorkflowConfiguration {
    let mut configuration = WorkflowConfiguration::default();
    *configuration.source_mut() = Source::Remote(RemoteSource::new(REMOTE_URL.into(), None, None));
    configuration
}

/// Commits `main.nf` with `content` to `repository` and returns the commit.
///
/// # Arguments
/// * `repository` - Non-bare repository
/// * `content` - New file content
/// * `parent` - Parent commit, `None` for the first commit
fn commit(repository: &Repository, content: &str, parent: Option<Oid>) -> Oid {
    std::fs::write(repository.workdir().unwrap().join("main.nf"), content).unwrap();
    let mut index = repository.index().unwrap();
    index.add_path(Path::new("main.nf")).unwrap();
    index.write().unwrap();
    let tree = repository.find_tree(index.write_tree().unwrap()).unwrap();
    let signature = Signature::now("t", "t@example.org").unwrap();
    let parents: Vec<_> = parent
        .map(|p| repository.find_commit(p).unwrap())
        .into_iter()
        .collect();
    let parents: Vec<_> = parents.iter().collect();
    repository
        .commit(Some("HEAD"), &signature, &signature, "c", &tree, &parents)
        .unwrap()
}

/// Creates a local repository and prepares the cache of `app` to fetch [`REMOTE_URL`] from it
/// (a `url.<path>.insteadOf` rewrite in the mirror), so no network is needed.
///
/// # Arguments
/// * `app` - Test application whose repository cache is prepared
/// * `directory` - Directory to create the repository in
fn local_remote(app: &TestApp, directory: &Path) -> Repository {
    let remote = Repository::init(directory.join("remote")).unwrap();
    let mirror = app
        .state
        .workflow_repository_cache()
        .root()
        .join("mirrors/local/o/r.git");
    std::fs::create_dir_all(mirror.parent().unwrap()).unwrap();
    Repository::init_bare(&mirror)
        .unwrap()
        .config()
        .unwrap()
        .set_str(
            &format!("url.{}.insteadOf", remote.workdir().unwrap().display()),
            REMOTE_URL,
        )
        .unwrap();
    remote
}

/// Polls the prefetch endpoint of `workflow_id` until it is no longer `fetching` and returns
/// the body.
///
/// # Arguments
/// * `app` - Test application
/// * `user` - An editor of the workflow
/// * `workflow_id` - The workflow
async fn finished_prefetch(app: &TestApp, user: &crate::models::User, workflow_id: i64) -> Value {
    let uri = format!("/workflow-integrations/prefetch/{workflow_id}");
    let mut body = Value::Null;
    for _ in 0..100 {
        (_, body) = app.send(Some(user), Method::GET, &uri, None).await;
        if body["state"] != "fetching" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    body
}

/// Saving a workflow with an unreachable remote source succeeds at once; the failed download is
/// reported by the prefetch endpoint, which only editors may read. Switching to a non-remote
/// source clears the state again.
#[tokio::test]
async fn saving_a_remote_workflow_prefetches_without_blocking() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let viewer = app.user("viewer").await;
    let workflow = app.workflow(&alice, "w").await;
    app.share::<crate::models::WorkflowIntegration>(
        workflow.id,
        crate::authorization::Principal::User(viewer.id),
        macworp_core::entities::Role::Viewer,
    )
    .await;
    let prefetch = format!("/workflow-integrations/prefetch/{}", workflow.id);
    let update = format!("/workflow-integrations/update/{}", workflow.id);

    let (status, body) = app.send(Some(&alice), Method::GET, &prefetch, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["state"], "idle");
    let (status, _) = app.send(Some(&viewer), Method::GET, &prefetch, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    let mut configuration = WorkflowConfiguration::default();
    *configuration.source_mut() = Source::Remote(RemoteSource::new(
        "https://prefetch.invalid/o/r".into(),
        Some("v1".into()),
        None,
    ));
    let (status, _) = app
        .send(
            Some(&alice),
            Method::POST,
            &update,
            Some(update_body(&configuration)),
        )
        .await;
    assert_eq!(status, StatusCode::OK);

    let mut state = Value::Null;
    for _ in 0..100 {
        let (_, body) = app.send(Some(&alice), Method::GET, &prefetch, None).await;
        state = body["state"].clone();
        if state != "fetching" {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    assert_eq!(state, "failed");

    let (status, _) = app
        .send(
            Some(&alice),
            Method::POST,
            &update,
            Some(update_body(&WorkflowConfiguration::default())),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let (_, body) = app.send(Some(&alice), Method::GET, &prefetch, None).await;
    assert_eq!(body["state"], "idle");
}

/// Saving a workflow with a reachable remote caches the repository and reports the commit;
/// deleting the workflow forgets the state.
#[tokio::test]
async fn saving_a_reachable_remote_workflow_reports_the_commit() {
    let app = TestApp::new().await;
    let temporary = tempfile::tempdir().unwrap();
    let remote = local_remote(&app, temporary.path());
    let first = commit(&remote, "1", None);
    let alice = app.user("alice").await;
    let workflow = app.workflow(&alice, "w").await;

    let (status, _) = app
        .send(
            Some(&alice),
            Method::POST,
            &format!("/workflow-integrations/update/{}", workflow.id),
            Some(update_body(&remote_configuration())),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    let body = finished_prefetch(&app, &alice, workflow.id).await;
    assert_eq!(body["state"], "done", "{body}");
    assert_eq!(body["commit"], first.to_string());

    let (status, _) = app
        .send(
            Some(&alice),
            Method::POST,
            &format!("/workflow-integrations/delete/{}", workflow.id),
            None,
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        app.state.prefetch_status(workflow.id),
        macworp_core::responses::PrefetchResponse::Idle
    );
}

/// The background refresh fetches a repository once however many workflows follow it, picks up
/// new commits of the default branch and skips workflows with other sources.
#[tokio::test]
async fn refresh_updates_each_repository_once() {
    let app = TestApp::new().await;
    let temporary = tempfile::tempdir().unwrap();
    let remote = local_remote(&app, temporary.path());
    let first = commit(&remote, "1", None);
    let alice = app.user("alice").await;
    let mut ids = Vec::new();
    for name in ["a", "b", "local"] {
        let workflow = app.workflow(&alice, name).await;
        if name != "local" {
            let (status, _) = app
                .send(
                    Some(&alice),
                    Method::POST,
                    &format!("/workflow-integrations/update/{}", workflow.id),
                    Some(update_body(&remote_configuration())),
                )
                .await;
            assert_eq!(status, StatusCode::OK);
            ids.push(workflow.id);
        }
    }
    for id in ids {
        assert_eq!(finished_prefetch(&app, &alice, id).await["state"], "done");
    }

    let second = commit(&remote, "2", Some(first));
    assert_eq!(app.state.refresh_workflow_repositories().await.unwrap(), 1);
    let shas: Vec<_> = app
        .state
        .workflow_repository_cache()
        .checkouts()
        .unwrap()
        .into_iter()
        .map(|checkout| checkout.sha)
        .collect();
    assert!(shas.contains(&second.to_string()), "{shas:?}");
}
