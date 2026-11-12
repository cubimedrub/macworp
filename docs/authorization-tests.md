# Authentication and authorization test matrix

Which automated test covers which authentication or authorization scenario, for reviews and audits. The rules themselves are in the README ("Permissions") and `docs/adr/0003-authz-model.md`.

**Keep this file in sync:** a new or changed rule needs a row here, and a renamed or deleted test needs its row updated. Gaps are listed at the end, not hidden.

Run everything (SQLite, no external services):

```bash
cargo test -p macworp_backend --lib --no-default-features --features integration
```

## Test levels

| Level | Where | What it exercises |
| --- | --- | --- |
| **Unit** | `authentication/jwt.rs` | Token encode/decode, secret validation |
| **Model** | `authorization/share_tests.rs`, `authorization/departure_tests.rs`, `models/user.rs` | Shares, visibility SQL and account-departure rules directly on the database, no HTTP |
| **`authorize`** | `authorization/mod.rs` (`tests`), `authorization/role_tests.rs` | The `authorize` function and extractors (`RequireAuth`, `ListScope`) with real tokens, no router |
| **Controller function** | `controllers/users_controller.rs` (`tests`) | Controller logic called as functions, without HTTP routing |
| **HTTP** | `controllers/authorization_http_tests.rs` | Full router (all controllers, auth extractors, JSON), driven with `tower::ServiceExt::oneshot`; fixtures in `test_support.rs` (`TestApp`) |

Test paths below are relative to the `macworp_backend` crate. Short names: `auth::tests` = `authorization::tests`, `http` = `controllers::authorization_http_tests`, `users` = `controllers::users_controller::tests`.

## 1. Authentication (who is calling)

| Scenario | Test | Level | Expected |
| --- | --- | --- | --- |
| Token round trip uses the default validity (no unlimited tokens) | `authentication::jwt::tests::roundtrip_uses_default_validity` | Unit | expiry = `DEFAULT_LOGIN_VALIDITY` |
| Token signed with another secret | `authentication::jwt::tests::wrong_secret_is_rejected` | Unit | `JWTDecryption`, no forging |
| Token whose subject is not a number | `authentication::jwt::tests::non_numeric_subject_is_error` | Unit | 401 `InvalidTokenSubject`, no panic |
| Weak or default JWT secret in configuration | `authentication::jwt::tests::weak_secrets_fail_validation` | Unit | validation fails for `"development"` and secrets < 32 bytes |
| Request without credentials | `http::anonymous_requests_are_rejected` | HTTP | 400 or 401 on projects, workflows, groups, user routes |
| Disabled account with a valid token | `auth::tests::disabled_user_token_is_rejected` | `authorize` | rejected (`AccountDisabled`), works again after re-enable |
| Token of a deleted account | `auth::tests::deleted_user_token_is_rejected` | `authorize` | 401 |
| Logout / revoked tokens (`token_version` bump) | `auth::tests::revoked_tokens_are_rejected` | `authorize` | old token 401, fresh token ok, other users unaffected |
| Worker (Basic auth) identity holds no shares | `auth::tests::worker_identity_has_no_access` | `authorize` | 404 on resources |
| Worker identity on user-facing routes (user, projects, workflows, groups, shares, runs, logout) | `authorization_http_tests::worker_is_rejected_on_user_routes` | HTTP | 403 `UserRequired` (`RequireUser`), wrong worker password 401, user JWT unaffected |
| Worker endpoints: full protocol with the per-run token | `worker_http_tests::claim_heartbeat_logs_progress_finish` | HTTP | claim 200 + token, heartbeat/logs/progress 2xx, finish stores status, token dead afterwards (401) |
| `claim` needs the worker credential | `worker_http_tests::claim_requires_worker_credential` | HTTP | user JWT, wrong password, run token 401, no header 400; run stays queued |
| Run token is scoped to its run; JWT and worker credential rejected on per-run endpoints | `worker_http_tests::run_token_is_scoped_to_its_run` | HTTP | other run's token 401, JWT 401, Basic 401 |
| Announce needs the worker credential, `/utilities/engines` needs a user | `worker_http_tests::announce_and_engines_authentication` | HTTP | JWT/run token 401 on announce, worker 403 and anonymous rejected on engines |
| Claim leases only runs of the worker's engines | `worker_http_tests::claim_skips_runs_of_other_engines` | HTTP | other/empty engine list 204, run stays queued, matching engine 200 |
| Cancel reaches the worker via heartbeat | `worker_http_tests::heartbeat_reports_cancel_request` | HTTP | `cancel_requested: true`, worker can finish `Cancelled` |
| Stale worker after lease expiry and requeue | `worker_http_tests::requeued_run_rejects_stale_worker`, `models::run::tests::expired_lease_requeues_and_invalidates_token` | HTTP, model | old token 401, new claim gets a new token |
| Run of a project without directory | `worker_http_tests::claim_fails_runs_of_projects_without_directory` | HTTP | run failed, claim returns 204 |
| Worker reports a failure: status `Failed` with reason (default `exit_code`) | `worker_http_tests::finish_failed_stores_reason` | HTTP | reason stored, run token is the only credential |

## 2. Visibility and roles (what a user may see and do)

| Scenario | Test | Level | Expected |
| --- | --- | --- | --- |
| Private by default | `authorization::share_tests::private_by_default` | Model | only the owner sees the project |
| Owner passes every role check | `auth::tests::owner_passes_every_role` | `authorize` | Viewer/Editor/Owner all pass |
| Invisible or missing resource | `auth::tests::invisible_resource_is_not_found_not_forbidden` | `authorize` | 404, never 403 (ids do not leak) |
| Visible but role too low | `auth::tests::viewer_is_forbidden_from_editing` | `authorize` | Viewer passes Viewer, 403 on Editor |
| Workflow shared to a user as Viewer / Editor / Owner | `authorization::role_tests::workflow_user_shares_per_role` | `authorize` | role granted exactly, higher minimums refused |
| Group share reaches members only; leaving the group removes access | `authorization::role_tests::group_share_follows_membership`, `authorization::share_tests::group_share_applies_to_members_only` | `authorize`, Model | member gets the role, outsider 404, ex-member 404 |
| Effective role = highest of user, group, public share | `authorization::share_tests::effective_role_is_the_maximum` | Model | maximum wins |
| Role change takes effect immediately (up and down) | `authorization::role_tests::role_change_takes_effect` | `authorize` | next check uses the new role |
| Running needs project Editor AND workflow Viewer | `auth::tests::public_workflow_and_editor_project_allow_running` | `authorize` | both independent |

## 3. Endpoint role matrix over HTTP

| Scenario | Test | Expected |
| --- | --- | --- |
| Project list: own + user/group/public shares only, no private ones of others, owner name shown, no `my_role` in lists | `http::project_list_is_scoped` | |
| Workflow list scoped the same way | `http::workflow_list_is_scoped` | |
| Project endpoints: edit = Viewer, create-folder / delete-path = Editor, delete = Owner (also removes shares) | `http::project_endpoints_need_their_role` | owner/admin/editor/viewer/outsider: 200 / 403 / 404 as per role; `my_role` set on single endpoint |
| Workflow endpoints: view = Viewer, update = Editor, delete = Owner (also removes shares) | `http::workflow_endpoints_need_their_role` | same matrix |
| Workflow schemas endpoint = Viewer (owner and shared viewer read, outsider 404, anonymous 400/401); content per source | `workflow_schemas_http_tests::remote_workflow_returns_its_schemas`, `local_workflow_returns_the_newest_upload`, `sources_without_schemas_and_unreachable_remotes` | HTTP | Nextflow/sample sheet/Snakemake schemas, commit for remote, newest upload for local, empty for others, 502 for an unreachable uncached repository |
| Workflow upload = Editor, prefetch state = Editor (a Viewer gets 403, an outsider 404) | `workflow_upload_http_tests::upload_installs_the_workflow`, `workflow_prefetch_http_tests::saving_a_remote_workflow_prefetches_without_blocking` | HTTP | |
| Run read endpoints (`index`, `get`, `logs`) = project Viewer; `cancel` = project Editor | `http::run_endpoints_need_their_role` | outsider 404, viewer cancel 403 |
| Workflow names of runs the caller cannot see are blanked | `http::run_endpoints_need_their_role` | empty `workflow_name` for the viewer |
| Run start: no project access 404, project Viewer 403, project Editor + invisible workflow 404, both roles pass | `http::run_start_needs_both_roles` | authorization passes (request may fail later) |
| Public Editor on project + public Viewer on workflow lets anyone run | `http::public_editor_project_allows_running` | authorization passes |
| Creating gives the creator an Owner share and `my_role = Owner` | `http::creator_becomes_owner` | one share row |
| A group created through the API lists its owner as member, so a group share reaches them; the owner cannot be removed | `http::group_owner_is_a_member` | |

### Global reference data

| Scenario | Test | Expected |
| --- | --- | --- |
| Every user can browse (role Viewer), files listed | `reference_http_tests::everybody_can_read` | 200, `my_role` Viewer |
| Anonymous rejected, unknown id invisible | `reference_http_tests::anonymous_and_unknown_id_are_rejected` | 400/401, 404 |
| Viewer cannot write; user Editor, group Editor and admin pass authorization | `reference_http_tests::writing_needs_editor_or_admin` | 403 / 405 (test filesystem is read-only) |
| Only admins manage shares; no Owner grant; public Viewer share cannot be changed or revoked | `reference_http_tests::share_rules` | 403 / 400 |
| Deleting an Editor keeps the data and everybody's read access | `reference_http_tests::user_deletion_keeps_reference_data` | 200, still visible |
| Symlinks in the folder are not followed | `reference_http_tests::symlinks_are_rejected` | 403 |
| Project browser shows the virtual `macworp_global_reference_data` folder (hides a real one), its content is read-only, projects cannot write there | `reference_http_tests::project_browser_shows_the_reference_folder` | `read_only` true, 400 on create-folder |
| `Path` values under the prefix become absolute reference paths, `..` is rejected, missing directory is an error | `render::tests::reference_prefix_is_rewritten_at_render` | |
| Run container mounts the folder read-only | `executors::nextflow::tests::reference_directory_is_mounted_read_only` | `-v dir:dir:ro` |

## 4. Sharing rules

| Scenario | Test | Level | Expected |
| --- | --- | --- | --- |
| Public share cap: workflow max Viewer, project up to Owner | `authorization::share_tests::public_role_is_capped`, `http::public_share_caps_apply_over_http` | Model, HTTP | 400 `PublicRoleTooHigh` for workflow Editor/Owner |
| Only Owners manage shares | `http::only_owners_manage_shares` | HTTP | Editor 403, Owner 200 |
| Grant/revoke checks the resource before the principal | `http::grant_checks_resource_before_principal` | HTTP | unknown principal: outsider 404, Viewer 403, Owner 404 (principal error) |
| Last owner cannot be demoted or removed | `authorization::share_tests::last_owner_is_protected` | Model | `LastOwner`; a second owner frees the first |
| Regrant replaces instead of duplicating | `authorization::share_tests::regrant_replaces_instead_of_duplicating` | Model | one row |
| Share listing is per resource | `authorization::share_tests::list_returns_grants_of_the_resource` | Model | |
| Deleting a group removes its shares everywhere | `authorization::share_tests::delete_group_shares_cleans_up` | Model | other principals untouched |
| Migration backfills creators as Owner | `authorization::share_tests::migration_backfills_existing_rows` | Model | |
| `ownerless` ignores viewers and public owners | `authorization::share_tests::ownerless_among_ignores_viewers_and_public_owners`, `http::ownerless_flag_is_reported` | Model, HTTP | flag in responses |

## 5. Administrator

| Scenario | Test | Level | Expected |
| --- | --- | --- | --- |
| Admin is Owner on unshared resources; missing rows stay 404 | `auth::tests::admin_is_owner_of_unshared_resources`, `authorization::role_tests::admin_owns_everything` | `authorize` | |
| Admin passes every project and workflow endpoint | `http::project_endpoints_need_their_role`, `http::workflow_endpoints_need_their_role` | HTTP | admin row in the matrix |
| `?all=true` lists everything for admins, 403 for others (projects, workflows, groups) | `http::show_all_is_admin_only`, `auth::tests::list_scope_all_is_admin_only` | HTTP, `authorize` | without the flag admin sees only own/shared |
| User administration (index, disable, enable, delete others) is admin-only | `http::user_administration_is_admin_only` | HTTP | non-admin 403; `current` open to any user |
| Create / update / disable / enable / reset password logic | `users::create_validates_and_hashes`, `users::update_protects_last_admin`, `users::disable_and_enable`, `users::reset_password_replaces_hash`, `users::list_filters` | Controller function | last enabled admin protected, no self-disable |
| Promote / demote via CLI model function | `models::user::tests::promote_and_demote`, `last_admin_cannot_be_demoted`, `unknown_user` | Model | last admin cannot be demoted |

## 6. Account deletion and orphan rules

| Scenario | Test | Level |
| --- | --- | --- |
| Delete user: sole-owned resources go through the orphan rules | `users::delete_applies_orphan_rules` | Controller function |
| Another owner left: oldest enabled owner becomes creator | `authorization::departure_tests::project_with_another_owner_goes_to_the_oldest_enabled_owner` | Model |
| Only a group owner left: stays owned, no creator shown | `authorization::departure_tests::project_with_only_a_group_owner_has_no_creator` | Model |
| Editors left: project becomes ownerless | `authorization::departure_tests::project_with_editors_becomes_ownerless` | Model |
| No owner and no editor: project and shares deleted | `authorization::departure_tests::project_without_owner_and_editor_is_deleted` | Model |
| Ownerless project losing its last editor is deleted | `authorization::departure_tests::ownerless_project_losing_its_last_editor_is_deleted` | Model |
| Workflow rules (viewer/editor keeps it ownerless, nobody deletes) | `authorization::departure_tests::workflow_rules` | Model |
| Group rules (ownerless if used and has members, else deleted) | `authorization::departure_tests::group_rules` | Model |
| Deleting a group re-settles resources it owned | `authorization::departure_tests::deleting_a_group_settles_resources_again` | Model |
| Runs keep no starter | `authorization::departure_tests::runs_keep_no_starter` | Model |

## Known gaps

Not covered by any test today (see PLAN.md P2 for the open items):

- Reference data: upload/create/delete with a writable filesystem (the test app uses the read-only desktop filesystem), a real run reading a reference file (no e2e yet), the remote worker storage check for the reference folder.

- Concurrent last-owner demotions: `share_tests::concurrent_demotions_keep_one_owner` checks that a demotion waits for the resource lock (SQLite only; not mutation-sensitive, SQLite's own write lock already serializes there). Postgres row-lock path untested.
- `upload-file` endpoint role check; successful run start (only the authorization outcome is asserted).
- Login endpoints (local password, OIDC), password change, brute-force or rate limiting.
- HTTP-level tests for the rest of the groups controller (only the owner-is-member rule is covered) and user delete (self-deletion, 409 `LastAdmin`).
- Postgres: all tests run on SQLite.
