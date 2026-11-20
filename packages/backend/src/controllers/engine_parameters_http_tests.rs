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
