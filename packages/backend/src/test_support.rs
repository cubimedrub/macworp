use std::sync::Arc;

use axum::{Extension, Router, body::Body};
use http::{Method, Request, StatusCode, header};
use macworp_core::{
    entities::Role,
    routing::{
        GroupsRouting, IsRouting, LoginRouting, ProjectsRouting, ReferenceRouting, RunsRouting,
        SharesRouting, UsersRouting, UtilitiesRouting, WorkerRouting, WorkflowIntegrationsRouting,
    },
    workflow_configuration::WorkflowConfiguration,
};
use macworp_database_migration::{Migrator, MigratorTrait};
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use serde_json::Value;
use tower::ServiceExt;

use crate::{
    authentication::jwt::JsonWebToken,
    authorization::{Principal, Resource},
    configuration::Configuration,
    controllers::*,
    filesystems::local_filesystem::LocalFilesystem,
    models::{
        ActiveGroup, ActiveProject, ActiveRun, ActiveUser, ActiveUsersGroups,
        ActiveWorkflowIntegration, Group, Project, Run, User, WorkflowIntegration,
    },
    state::State,
};

/// Username of the worker credential configured in [`TestApp`]
pub const WORKER_USERNAME: &str = "test-worker";
/// Password of the worker credential configured in [`TestApp`]
pub const WORKER_PASSWORD: &str = "test-worker-password";

/// A migrated SQLite database in a temp dir plus the full API router, driven in-process with
/// `tower::ServiceExt::oneshot` (no socket). Shared by the HTTP-level tests.
pub struct TestApp {
    /// State the router was built from
    pub state: Arc<State>,
    /// Router with all controllers nested at their routing paths
    router: Router,
    /// Keeps the database and project directories alive
    pub directory: tempfile::TempDir,
}

impl TestApp {
    /// Creates the database, runs the migrations and builds the router.
    pub async fn new() -> Self {
        Self::new_with(|_| {}).await
    }

    /// Like [`Self::new`], with per-run reference snapshots turned on (as in the standalone
    /// backend) below `<storage>/reference_snapshots`; local workflow paths are allowed so a run
    /// can start without an uploaded workflow.
    pub async fn new_with_reference_snapshots() -> Self {
        Self::build(|c| *c.allow_local_paths_mut() = true, true).await
    }

    /// Like [`Self::new`], with a container runtime the `/runtime` routes forward to.
    ///
    /// # Arguments
    /// * `control` - the runtime of the test
    pub async fn new_with_runtime_control(
        control: Arc<dyn crate::runtime_control::RuntimeControl>,
    ) -> Self {
        Self::build_with(|_| {}, false, Some(control)).await
    }

    /// Like [`Self::new`], but lets `adjust` change the configuration first.
    ///
    /// # Arguments
    /// * `adjust` - Called with the default test configuration before the backend state is built
    pub async fn new_with(adjust: impl FnOnce(&mut Configuration)) -> Self {
        Self::build(adjust, false).await
    }

    /// Builds the app.
    ///
    /// # Arguments
    /// * `adjust` - Called with the default test configuration before the backend state is built
    /// * `reference_snapshots` - Whether runs get a snapshot of the reference data
    async fn build(adjust: impl FnOnce(&mut Configuration), reference_snapshots: bool) -> Self {
        Self::build_with(adjust, reference_snapshots, None).await
    }

    /// Builds the app.
    ///
    /// # Arguments
    /// * `adjust` - Called with the default test configuration before the backend state is built
    /// * `reference_snapshots` - Whether runs get a snapshot of the reference data
    /// * `runtime_control` - The container runtime served under `/runtime`, if any
    async fn build_with(
        adjust: impl FnOnce(&mut Configuration),
        reference_snapshots: bool,
        runtime_control: Option<Arc<dyn crate::runtime_control::RuntimeControl>>,
    ) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let mut configuration = Configuration::default();
        *configuration.worker_mut() = crate::configuration::WorkerCredentials::new(
            WORKER_USERNAME.into(),
            password_auth::generate_hash(WORKER_PASSWORD.as_bytes()),
        );
        *configuration.database_url_mut() =
            format!("sqlite://{}/test.db?mode=rwc", directory.path().display());
        *configuration.storage_path_mut() = directory.path().join("projects").display().to_string();
        adjust(&mut configuration);
        let state = State::new_for_integration(configuration).await.unwrap();
        state
            .reference_directory()
            .set(directory.path().join("reference"));
        let state = if reference_snapshots {
            let root = directory.path().join("reference_snapshots");
            state.with_reference_snapshots(root)
        } else {
            state
        };
        let state = match runtime_control {
            Some(control) => state.with_runtime_control(control),
            None => state,
        };
        std::fs::create_dir_all(directory.path().join("reference")).unwrap();
        Migrator::up(state.db(), None).await.unwrap();
        let state = Arc::new(state);

        let router = Router::new()
            .nest(
                UtilitiesRouting::controller_path(),
                UtilitiesController::routes(),
            )
            .nest(
                LoginRouting::controller_path(),
                LoginController::routes(state.clone()),
            )
            .nest(
                UsersRouting::controller_path(),
                UsersController::routes::<LocalFilesystem>(state.clone()),
            )
            .nest(
                ProjectsRouting::controller_path(),
                ProjectsController::<LocalFilesystem>::routes(state.clone()),
            )
            .nest(
                ReferenceRouting::controller_path(),
                ReferenceController::<LocalFilesystem>::routes(state.clone()),
            )
            .nest(
                WorkflowIntegrationsRouting::controller_path(),
                WorkflowIntegrationsController::routes(state.clone()),
            )
            .nest(
                RunsRouting::controller_path(),
                RunsController::routes(state.clone()),
            )
            .nest(
                GroupsRouting::controller_path(),
                GroupsController::routes(state.clone()),
            )
            .nest(
                SharesRouting::controller_path(),
                SharesController::routes(state.clone()),
            )
            .nest(
                WorkerRouting::controller_path(),
                WorkerController::<LocalFilesystem>::routes(state.clone()),
            )
            .nest(
                macworp_core::routing::RuntimeRouting::controller_path(),
                RuntimeController::routes(state.clone()),
            )
            .layer(Extension(state.clone()))
            .with_state(state.clone());

        Self {
            state,
            router,
            directory,
        }
    }

    /// Inserts a regular user named `name`.
    pub async fn user(&self, name: &str) -> User {
        ActiveUser {
            username: Set(name.into()),
            firstname: Set(name.into()),
            lastname: Set(name.into()),
            email: Set(format!("{name}@example.org")),
            ..Default::default()
        }
        .insert(self.state.db())
        .await
        .unwrap()
    }

    /// Inserts an administrator named `name`.
    pub async fn admin(&self, name: &str) -> User {
        let mut user: ActiveUser = self.user(name).await.into();
        user.is_admin = Set(true);
        user.update(self.state.db()).await.unwrap()
    }

    /// Inserts a project whose directory exists below the temp dir, with `owner` as Owner.
    pub async fn project(&self, owner: &User, name: &str) -> Project {
        let path = self.directory.path().join(format!("project-{name}"));
        std::fs::create_dir_all(&path).unwrap();
        let project = ActiveProject {
            owner_id: Set(Some(owner.id)),
            name: Set(name.into()),
            path: Set(Some(path.display().to_string())),
            ..Default::default()
        }
        .insert(self.state.db())
        .await
        .unwrap();
        Project::shares(self.state.db(), project.id)
            .grant(Principal::User(owner.id), Role::Owner)
            .await
            .unwrap();
        project
    }

    /// Inserts a project whose directory lies below the storage root (`<storage>/projects/<name>`),
    /// like projects of a standalone server, with `owner` as Owner.
    ///
    /// # Arguments
    /// * `owner` - Creator and Owner
    /// * `name` - Project name, also its directory name
    pub async fn project_in_storage(&self, owner: &User, name: &str) -> Project {
        let path = self.state.config().projects_directory().join(name);
        std::fs::create_dir_all(&path).unwrap();
        let project = ActiveProject {
            owner_id: Set(Some(owner.id)),
            name: Set(name.into()),
            path: Set(Some(path.display().to_string())),
            ..Default::default()
        }
        .insert(self.state.db())
        .await
        .unwrap();
        Project::shares(self.state.db(), project.id)
            .grant(Principal::User(owner.id), Role::Owner)
            .await
            .unwrap();
        project
    }

    /// Inserts a workflow with `owner` as Owner.
    pub async fn workflow(&self, owner: &User, name: &str) -> WorkflowIntegration {
        let workflow = ActiveWorkflowIntegration {
            owner_id: Set(Some(owner.id)),
            name: Set(name.into()),
            description: Set(String::new()),
            ..Default::default()
        }
        .insert(self.state.db())
        .await
        .unwrap();
        WorkflowIntegration::shares(self.state.db(), workflow.id)
            .grant(Principal::User(owner.id), Role::Owner)
            .await
            .unwrap();
        workflow
    }

    /// Inserts a queued run of `workflow` on `project`.
    pub async fn run(&self, project: &Project, workflow: &WorkflowIntegration) -> Run {
        ActiveRun {
            project_id: Set(project.id),
            workflow_id: Set(Some(workflow.id)),
            configuration: Set(WorkflowConfiguration::default()),
            parameters: Set(Value::Object(Default::default())),
            comment: Set(String::new()),
            cancel_requested: Set(false),
            locked: Set(false),
            created_at: Set(chrono::Utc::now()),
            ..Default::default()
        }
        .insert(self.state.db())
        .await
        .unwrap()
    }

    /// Inserts a group owned by `owner` with `members`; the owner is a member too, as for groups
    /// created through the API (listing them in `members` as well is harmless).
    pub async fn group(&self, owner: &User, name: &str, members: &[&User]) -> Group {
        let group = ActiveGroup {
            owner_id: Set(Some(owner.id)),
            name: Set(name.into()),
            description: Set(String::new()),
            ..Default::default()
        }
        .insert(self.state.db())
        .await
        .unwrap();
        let mut everyone = vec![owner];
        for member in members {
            if member.id != owner.id {
                everyone.push(member);
            }
        }
        for member in everyone {
            ActiveUsersGroups {
                user_id: Set(member.id),
                group_id: Set(group.id),
            }
            .insert(self.state.db())
            .await
            .unwrap();
        }
        group
    }

    /// Sends a request as `user` (anonymous for `None`) and returns the status and JSON body
    /// (`Value::Null` when the body is empty or not JSON).
    pub async fn send(
        &self,
        user: Option<&User>,
        method: Method,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let authorization = user.map(|user| {
            let token =
                JsonWebToken::encode(user.id, user.token_version, self.state.config()).unwrap();
            format!("JWT {token}")
        });
        self.send_authorized(authorization, method, uri, body).await
    }

    /// Sends a request as the worker (Basic authentication with `password`).
    pub async fn send_as_worker(
        &self,
        password: &str,
        method: Method,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        use base64::prelude::*;
        let credentials = BASE64_STANDARD.encode(format!("{WORKER_USERNAME}:{password}"));
        self.send_authorized(Some(format!("Basic {credentials}")), method, uri, body)
            .await
    }

    /// Sends a request with the given `Authorization` header value (e.g. `Run <token>`) and
    /// returns the status and JSON body.
    pub async fn send_authorized(
        &self,
        authorization: Option<String>,
        method: Method,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut request = Request::builder().method(method).uri(uri);
        if let Some(authorization) = authorization {
            request = request.header(header::AUTHORIZATION, authorization);
        }
        let request = match body {
            Some(body) => request
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        }
        .unwrap();

        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    /// Sends a request with a raw (binary) body as `user` and returns the status and JSON body.
    ///
    /// # Arguments
    /// * `user` - Authenticated user
    /// * `method` - HTTP method
    /// * `uri` - Request path with query
    /// * `body` - Raw request body
    pub async fn send_bytes(
        &self,
        user: &User,
        method: Method,
        uri: &str,
        body: Vec<u8>,
    ) -> (StatusCode, Value) {
        let token = JsonWebToken::encode(user.id, user.token_version, self.state.config()).unwrap();
        let request = Request::builder()
            .method(method)
            .uri(uri)
            .header(header::AUTHORIZATION, format!("JWT {token}"))
            .header(header::CONTENT_TYPE, "application/zip")
            .body(Body::from(body))
            .unwrap();
        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    /// `GET uri` as `user` (anonymous for `None`), returning status, headers and the raw body.
    ///
    /// # Arguments
    /// * `user` - Authenticated user, or `None` to send no `Authorization` header
    /// * `uri` - Request path with query
    pub async fn get_raw(
        &self,
        user: Option<&User>,
        uri: &str,
    ) -> (StatusCode, http::HeaderMap, Vec<u8>) {
        let mut request = Request::builder().method(Method::GET).uri(uri);
        if let Some(user) = user {
            let token =
                JsonWebToken::encode(user.id, user.token_version, self.state.config()).unwrap();
            request = request.header(header::AUTHORIZATION, format!("JWT {token}"));
        }
        let response = self
            .router
            .clone()
            .oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let (parts, body) = response.into_parts();
        let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
        (parts.status, parts.headers, bytes.to_vec())
    }

    /// Obtains a one-time token for `user` through the API.
    ///
    /// # Arguments
    /// * `user` - The user to authenticate as
    pub async fn one_time_token(&self, user: &User) -> String {
        let (status, body) = self.post(user, "/user/one-time-token", None).await;
        assert_eq!(status, StatusCode::OK);
        body["token"].as_str().unwrap().to_string()
    }

    /// `GET uri` as `user`.
    pub async fn get(&self, user: &User, uri: &str) -> (StatusCode, Value) {
        self.send(Some(user), Method::GET, uri, None).await
    }

    /// `POST uri` as `user` with an optional JSON body.
    pub async fn post(&self, user: &User, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
        self.send(Some(user), Method::POST, uri, body).await
    }

    /// Grants `role` on `resource` to `principal` directly in the database.
    pub async fn share<R: Resource>(&self, id: i64, principal: Principal, role: Role) {
        R::shares(self.state.db(), id)
            .grant(principal, role)
            .await
            .unwrap();
    }
}

/// Whether `status` means the authorization check passed (the request may still fail later for
/// unrelated reasons such as a missing file or invalid parameters).
pub fn passed_authorization(status: StatusCode) -> bool {
    status != StatusCode::FORBIDDEN && status != StatusCode::NOT_FOUND
}
