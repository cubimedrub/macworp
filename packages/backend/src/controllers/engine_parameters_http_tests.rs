use http::StatusCode;
use macworp_core::workflow_configuration::{WorkflowConfiguration, engine::EngineParameter};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, IntoActiveModel};
use serde_json::json;

use crate::test_support::TestApp;

/// Configuration with the given engine parameters.
///
/// # Arguments
/// * `parameters` - `(name, value)` pairs, an empty value is a switch
fn configuration(parameters: &[(&str, &str)]) -> WorkflowConfiguration {
    let mut configuration = WorkflowConfiguration::default();
    *configuration.engine_parameters_mut() = parameters
        .iter()
        .map(|(name, value)| EngineParameter::new((*name).into(), (*value).into()))
        .collect();
    configuration
}

/// Saving a workflow stores allowed engine parameters and rejects every other one with 400:
/// managed ones (`resume`, `with-weblog`), unknown names, bad values and duplicates.
#[tokio::test]
async fn saving_checks_engine_parameters() {
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let workflow = app.workflow(&alice, "w").await;
    let uri = format!("/workflow-integrations/update/{}", workflow.id);
    let save = |parameters: &[(&str, &str)]| {
        let body =
            json!({"name": "w", "description": "", "configuration": configuration(parameters)});
        let (app, alice, uri) = (&app, &alice, &uri);
        async move { app.post(alice, uri, Some(body)).await.0 }
    };

    assert_eq!(
        save(&[("profile", "docker,test"), ("with-trace", "")]).await,
        StatusCode::OK
    );
    for rejected in [
        &[("resume", "")][..],
        &[("with-weblog", "http://evil")],
        &[("c", "/etc/nextflow.config")],
        &[("profile", "-c")],
        &[("with-trace", ""), ("with-trace", "")],
    ] {
        assert_eq!(
            save(rejected).await,
            StatusCode::BAD_REQUEST,
            "{rejected:?}"
        );
    }
}

/// A stored workflow with a disallowed parameter (saved before the allowlist existed) cannot
/// be started: 400 instead of a run that fails in the worker.
#[tokio::test]
async fn starting_checks_engine_parameters() {
    let app = TestApp::new_with(|c| *c.fixed_unavailable_engines_mut() = Some(Vec::new())).await;
    let alice = app.user("alice").await;
    let project = app.project(&alice, "p").await;
    let workflow = app.workflow(&alice, "w").await;
    let mut active = workflow.clone().into_active_model();
    active.configuration = Set(configuration(&[("work-dir", "/")]));
    active.update(app.state.db()).await.unwrap();

    let (status, body) = app
        .post(
            &alice,
            &format!("/runs/start/{}", project.id),
            Some(json!({"workflow_id": workflow.id, "comment": "", "parameters": []})),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
}

/// Users can switch trace, report, timeline and task graph at start; everything that changes
/// the workflow's behaviour (profile, error handling, resume, ...) is refused with 400.
#[tokio::test]
async fn starting_applies_only_observability_switches() {
    let app = TestApp::new_with(|c| *c.fixed_unavailable_engines_mut() = Some(Vec::new())).await;
    let alice = app.user("alice").await;
    let project = app.project(&alice, "p").await;
    let workflow = app.workflow(&alice, "w").await;
    let mut active = workflow.clone().into_active_model();
    active.configuration = Set(configuration(&[("profile", "test"), ("with-trace", "")]));
    active.update(app.state.db()).await.unwrap();

    let start = |name: &str, enabled: bool| {
        let body = json!({
            "workflow_id": workflow.id,
            "comment": "",
            "parameters": [],
            "engine_switches": [{"name": name, "enabled": enabled}],
        });
        let (app, alice, uri) = (&app, &alice, format!("/runs/start/{}", project.id));
        async move { app.post(alice, &uri, Some(body)).await.0 }
    };
    for name in ["profile", "queue-size", "resume", "with-weblog"] {
        assert_eq!(start(name, true).await, StatusCode::BAD_REQUEST, "{name}");
    }
    // Passes the engine parameter checks (a later step may still refuse the run)
    assert_ne!(start("with-dag", true).await, StatusCode::BAD_REQUEST);
    assert_ne!(start("with-trace", false).await, StatusCode::BAD_REQUEST);
}

/// Saving a workflow checks its image builds: a good mapping is stored, names that look like
/// official or registry images, escaping build files and duplicates are rejected with 400.
#[tokio::test]
async fn saving_checks_image_builds() {
    use macworp_core::workflow_configuration::images::ImageBuild;
    let app = TestApp::new().await;
    let alice = app.user("alice").await;
    let workflow = app.workflow(&alice, "w").await;
    let uri = format!("/workflow-integrations/update/{}", workflow.id);
    let save = |images: &[(&str, &str)]| {
        let mut configuration = WorkflowConfiguration::default();
        *configuration.images_mut() = images
            .iter()
            .map(|(file, name)| ImageBuild::new((*file).into(), (*name).into()))
            .collect();
        let body = json!({"name": "w", "description": "", "configuration": configuration});
        let (app, alice, uri) = (&app, &alice, &uri);
        async move { app.post(alice, uri, Some(body)).await.0 }
    };

    assert_eq!(
        save(&[("docker/Dockerfile", "myteam/tool:1.0")]).await,
        StatusCode::OK
    );
    for rejected in [
        &[("Dockerfile", "debian:12-slim")][..],
        &[("Dockerfile", "library/debian:12")],
        &[("Dockerfile", "ghcr.io/o/t:1")],
        &[("Dockerfile", "a/b:latest")],
        &[("../Dockerfile", "a/b:1")],
        &[("/etc/Dockerfile", "a/b:1")],
        &[("a/Dockerfile", "a/b:1"), ("b/Dockerfile", "a/b:1")],
    ] {
        assert_eq!(
            save(rejected).await,
            StatusCode::BAD_REQUEST,
            "{rejected:?}"
        );
    }
}
