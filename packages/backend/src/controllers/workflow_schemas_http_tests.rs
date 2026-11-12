use std::io::Write;
use std::path::Path;

use git2::{Oid, Repository, Signature};
use http::{Method, StatusCode};
use macworp_core::workflow_configuration::{
    WorkflowConfiguration,
    source::{ImageSource, RemoteSource, Source},
};
use serde_json::{Value, json};

use crate::test_support::TestApp;

/// URL the tests give their workflows; the mirror rewrites it to a local repository.
const REMOTE_URL: &str = "https://local/o/r";

/// A zip with the given `(name, content)` files.
///
/// # Arguments
/// * `files` - File names and contents
fn zip_of(files: &[(&str, &str)]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, content) in files {
        writer
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(content.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

/// Update body for a workflow with the given source.
///
/// # Arguments
/// * `source` - Source to store, the rest of the configuration is the default
fn update_body(source: Source) -> Value {
    let mut configuration = WorkflowConfiguration::default();
    *configuration.source_mut() = source;
    json!({"name": "w", "description": "", "configuration": configuration})
}

/// Commits the given files to `repository` as one commit on `HEAD`.
///
/// # Arguments
/// * `repository` - Non-bare repository
/// * `files` - `(path, content)` pairs, folders are created
/// * `parent` - Parent commit, `None` for the first commit
fn commit(repository: &Repository, files: &[(&str, &str)], parent: Option<Oid>) -> Oid {
    let mut index = repository.index().unwrap();
    for (path, content) in files {
        let absolute = repository.workdir().unwrap().join(path);
        std::fs::create_dir_all(absolute.parent().unwrap()).unwrap();
        std::fs::write(&absolute, content).unwrap();
        index.add_path(Path::new(path)).unwrap();
    }
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

/// Saves `source` for `workflow_id` as `user`.
///
/// # Arguments
/// * `app` - Test application
/// * `user` - An editor of the workflow
/// * `workflow_id` - Workflow to change
/// * `source` - New source
async fn save_source(app: &TestApp, user: &crate::models::User, workflow_id: i64, source: Source) {
    let (status, body) = app
        .post(
            user,
            &format!("/workflow-integrations/update/{workflow_id}"),
            Some(update_body(source)),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

/// `GET` of the schemas endpoint for `workflow_id` as `user`.
///
/// # Arguments
/// * `app` - Test application
/// * `user` - Acting user
/// * `workflow_id` - Workflow to read the schemas of
async fn schemas(
    app: &TestApp,
    user: &crate::models::User,
    workflow_id: i64,
) -> (StatusCode, Value) {
    app.get(
        user,
        &format!("/workflow-integrations/schemas/{workflow_id}"),
    )
    .await
}

/// Uploads `files` as the local workflow `workflow_id` (script `main.nf`) as `user`.
///
/// # Arguments
/// * `app` - Test application
/// * `user` - An editor of the workflow
/// * `workflow_id` - Workflow to upload to
/// * `files` - `(path, content)` pairs of the archive
async fn upload(
    app: &TestApp,
    user: &crate::models::User,
    workflow_id: i64,
    files: &[(&str, &str)],
) {
    let (status, body) = app
        .send_bytes(
            user,
            Method::POST,
            &format!("/workflow-integrations/upload/{workflow_id}"),
            zip_of(files),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
}

/// The schemas of a remote workflow come from the cached checkout: the Nextflow schema, the
/// sample sheet schema at the default path, and the commit they were read from. Viewers may
/// read them, outsiders and anonymous users may not.
#[tokio::test]
async fn remote_workflow_returns_its_schemas() {
    let app = TestApp::new().await;
    let temporary = tempfile::tempdir().unwrap();
    let remote = local_remote(&app, temporary.path());
    let first = commit(
        &remote,
        &[
            ("main.nf", "x"),
            ("nextflow_schema.json", r#"{"title": "pipeline"}"#),
            (
                "assets/schema_input.json",
                r#"{"items": {"type": "object"}}"#,
            ),
        ],
        None,
    );
    let alice = app.user("alice").await;
    let viewer = app.user("viewer").await;
    let outsider = app.user("outsider").await;
    let workflow = app.workflow(&alice, "w").await;
    app.share::<crate::models::WorkflowIntegration>(
        workflow.id,
        crate::authorization::Principal::User(viewer.id),
        macworp_core::entities::Role::Viewer,
    )
    .await;
    save_source(
        &app,
        &alice,
        workflow.id,
        Source::Remote(RemoteSource::new(REMOTE_URL.into(), None, None)),
    )
    .await;

    for user in [&alice, &viewer] {
        let (status, body) = schemas(&app, user, workflow.id).await;
        assert_eq!(status, StatusCode::OK, "{} {body}", user.username);
        assert_eq!(body["commit"], first.to_string());
        assert_eq!(body["nextflow_schema"], json!({"title": "pipeline"}));
        assert_eq!(body["input_schema"], json!({"items": {"type": "object"}}));
        assert_eq!(body["snakemake_schemas"], json!([]));
    }
    let (status, _) = schemas(&app, &outsider, workflow.id).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = app
        .send(
            None,
            Method::GET,
            &format!("/workflow-integrations/schemas/{}", workflow.id),
            None,
        )
        .await;
    // Like on every route: a missing token is rejected before any authorization check
    assert!(
        status == StatusCode::BAD_REQUEST || status == StatusCode::UNAUTHORIZED,
        "anonymous: {status}"
    );
}

/// The sample sheet schema is found where `properties.input.schema` of the Nextflow schema
/// points, also inside a `$defs` group as nf-core pipelines write it.
#[tokio::test]
async fn input_schema_follows_the_nextflow_schema() {
    let app = TestApp::new().await;
    let temporary = tempfile::tempdir().unwrap();
    let remote = local_remote(&app, temporary.path());
    commit(
        &remote,
        &[
            ("main.nf", "x"),
            (
                "nextflow_schema.json",
                r#"{"$defs": {"io": {"properties": {"input": {"schema": "assets/samples.json"}}}}}"#,
            ),
            ("assets/samples.json", r#"{"custom": true}"#),
            ("assets/schema_input.json", r#"{"custom": false}"#),
        ],
        None,
    );
    let alice = app.user("alice").await;
    let workflow = app.workflow(&alice, "w").await;
    save_source(
        &app,
        &alice,
        workflow.id,
        Source::Remote(RemoteSource::new(REMOTE_URL.into(), None, None)),
    )
    .await;

    let (status, body) = schemas(&app, &alice, workflow.id).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["input_schema"], json!({"custom": true}));
}

/// Schemas of an uploaded workflow have no commit, come from the newest upload, and Snakemake
/// schema files are returned as text.
#[tokio::test]
async fn local_workflow_returns_the_newest_upload() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let workflow = app.workflow(&alice, "w").await;

    // Nothing uploaded yet: empty answer
    let (status, body) = schemas(&app, &alice, workflow.id).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["commit"], Value::Null);
    assert_eq!(body["nextflow_schema"], Value::Null);

    upload(
        &app,
        &alice,
        workflow.id,
        &[
            ("main.nf", "x"),
            ("nextflow_schema.json", r#"{"version": 1}"#),
            ("workflow/schemas/config.schema.yaml", "type: object\n"),
        ],
    )
    .await;
    let (status, body) = schemas(&app, &alice, workflow.id).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["commit"], Value::Null);
    assert_eq!(body["nextflow_schema"], json!({"version": 1}));
    assert_eq!(
        body["snakemake_schemas"],
        json!([{"path": "workflow/schemas/config.schema.yaml", "content": "type: object\n"}])
    );

    upload(
        &app,
        &alice,
        workflow.id,
        &[
            ("main.nf", "x"),
            ("nextflow_schema.json", r#"{"version": 2}"#),
        ],
    )
    .await;
    let (_, body) = schemas(&app, &alice, workflow.id).await;
    assert_eq!(body["nextflow_schema"], json!({"version": 2}));
    assert_eq!(body["snakemake_schemas"], json!([]));
}

/// Sources without schema files answer with an empty response, an unreachable repository that
/// is not cached is a 502.
#[tokio::test]
async fn sources_without_schemas_and_unreachable_remotes() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let workflow = app.workflow(&alice, "w").await;

    save_source(
        &app,
        &alice,
        workflow.id,
        Source::Image(ImageSource::default()),
    )
    .await;
    let (status, body) = schemas(&app, &alice, workflow.id).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["nextflow_schema"], Value::Null);
    assert_eq!(body["input_schema"], Value::Null);
    assert_eq!(body["snakemake_schemas"], json!([]));

    save_source(
        &app,
        &alice,
        workflow.id,
        Source::Remote(RemoteSource::new(
            "https://schemas.invalid/o/r".into(),
            Some("v1".into()),
            None,
        )),
    )
    .await;
    let (status, _) = schemas(&app, &alice, workflow.id).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
}
