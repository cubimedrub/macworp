# MAcWorP - Massive aCcessible Workflow Platform

MaCWorP is a web application to give workflow developers a simple way to make their workflows easily accessible via a web browser for everyone. 

## Introduction
* What it **does** for you
    * Graphical user interface for
        * Data up- & download
        * Parameter input
        * Rendered results (interactive plots, PDFs, images, tables, ...)
    * Authentication
        * Simple file based authentication (easy to set up, recommended for smaller teams, labs or institutes)
        * OpenIDConnect (e.g. connect to ELIXIR's Life Science Login)
    * Distributed execution of a workflow on different workers
    * Coming soon(ish):
        * Support for multiple workflow engines (currently Nextflow is working)
        * Fine grained access control to projects and workflows including sharing with others.
* What it **does not** for you
    * Scaling - This is still the job of the workflow engine. E.g. you can configure Nextflow to use K8s or Slurm when started by MAcWorP workers but setting these executors up is still a separate job for an admin.

## Supported engines per container runtime
Engines run inside containers, so a container runtime (Podman or Docker) is required.

| Runtime | Nextflow | Snakemake | CWL | Container |
| --- | --- | --- | --- | --- |
| Docker | yes | yes | yes | yes |
| Podman | yes | no | yes | yes |
| WSL Containers (`wslc`, Windows) | no | no | no | yes |

Snakemake needs Docker. CWL support is still planned.

## Permissions

Which test covers which scenario: [docs/authorization-tests.md](docs/authorization-tests.md).

Projects and workflows are private: only users (or groups) they are shared with can see them. A user's role on a resource is the highest of the roles granted to them directly, to a group they belong to and to everybody (public). The sharing UI exists on the web deployment only (the backend routes are mounted in both builds); the desktop app has a single local user who owns everything.

Roles on a **project** or **workflow** (each includes the rights of the roles to its left):

| Action | Viewer | Editor | Owner |
| --- | :---: | :---: | :---: |
| **Project**: see it in lists, open it, browse its files and runs, read run logs | yes | yes | yes |
| Project: upload files, create folders, delete files and folders | | yes | yes |
| Project: start runs (also needs at least Viewer on the workflow) | | yes | yes |
| Project: cancel runs (only the project role is checked) | | yes | yes |
| Project: delete it | | | yes |
| Project: manage shares (users, groups, public) | | | yes |
| **Workflow**: see it, select it to run it on a project | yes | yes | yes |
| Workflow: edit its definition (engine, source, parameters) | | yes | yes |
| Workflow: delete it | | | yes |
| Workflow: manage shares (users, groups, public) | | | yes |

- Running needs both rights: Editor or Owner on the project and Viewer or higher on the workflow. A role on a workflow gives no rights on anybody's project.
- The public (everybody) can hold at most Viewer on a workflow; on a project it can hold any role. Editor and Owner on a workflow go to users or groups.
- While sharing, a resource always keeps at least one user or group Owner: the last one cannot be removed or demoted. Only deleting an account can leave a resource without an owner, see below.
- A resource the user cannot see behaves as if it does not exist (404); one they can see but lack the role for is refused (403).
- Granting a role to a group is only possible for groups the granting user owns or belongs to.

**Global reference data** (genomes, indices, annotation) is one instance-wide folder every project can use. It has no owner and cannot be deleted:

| Action | Any logged-in user | Editor | Admin |
| --- | :---: | :---: | :---: |
| Browse and read the files (also as the `macworp_global_reference_data` folder in every project's file browser and the file picker of `Path` parameters) | yes | yes | yes |
| Upload files, create folders, delete files and folders (web; on the desktop use the file manager) | | yes | yes |
| Make users or groups Editor (admins only; Owner cannot be granted, the public read access cannot be removed) | | | yes |

Workflows get the folder mounted read-only at its own path; a `Path` value under `macworp_global_reference_data/` is rewritten to that absolute path when the run starts, so projects cannot use that name for their own top-level folder. On the desktop the user picks the folder in **Settings** (default: `global_reference_data` in the application data directory); on a server it is `global_reference_data` below the projects directory.

Other actions:

| Action | Any logged-in user | Group member | Group owner | Admin |
| --- | :---: | :---: | :---: | :---: |
| Create projects, workflows and groups (the creator becomes Owner) | yes | | | yes |
| Search users by username (to share or add members) | yes | | | yes |
| See a group and its members | | yes | yes | yes |
| Rename or delete a group, add or remove members | | | yes | yes |
| Be named as share principal through a group | | yes | yes | yes (any group) |
| Read, edit, delete and share any project or workflow | | | | yes |
| List all projects, workflows and groups | | | | yes |
| Create, edit, disable users and reset passwords | | | | yes |
| Delete a user account (any user may delete their own) | own account | | | yes |

**Admin:** users have an `is_admin` flag (set with `macworp_backend user promote|demote <username>` or by another admin). An admin has full create, read, update and delete rights on all projects, workflows, groups and users, can list everything (the "Show all" switch above the project, workflow and group lists, web only) and can disable users and reset passwords. The last enabled admin cannot be demoted, disabled or deleted, and admins cannot disable their own account. Admin actions are logged (tracing target `audit`): every admin user action (including on their own account), admin use of rights beyond their shares, `?all=true` listings and account deletions. A project, workflow or group nobody owns anymore is marked **ownerless** in lists and on its page; an admin can assign a new owner.

### Accounts and API behaviour

- Logging out invalidates all tokens of the user.
- A disabled user can neither log in nor use existing tokens (401). Their shares and group memberships stay.
- User search needs at least 2 characters, matches the username prefix and returns at most 10 users.
- `?all=true` on list endpoints is refused for non-admins (403).
- Sharing errors: public role above the allowed maximum 400, removing or demoting the last owner 409, unknown principal 404.

### Deleting an account

Every user may delete their own account, administrators may delete any account. It cannot be undone. The user's shares and group memberships are removed, then for every project, workflow and group they touched:

| Case | Project | Workflow | Group |
| --- | --- | --- | --- |
| Another user or group still owns it | stays; that owner takes over (the owner with the oldest share is shown as creator) | same | not applicable, a group has one owner |
| Nobody owns it, but somebody can still use it | stays **without an owner** while a user, group or the public can edit it (public Owner counts as Editor) | stays without an owner while somebody can view or edit it | stays without an owner while a share uses it and it has members |
| Nobody is left who can use it | deleted with its files and shares | deleted with its shares | deleted, together with its shares |

- Viewers alone do not keep a project alive, as nobody could maintain it.
- A group whose owner is deleted is removed when it has no members or no share uses it; a group that still has an owner stays, even without members. Deleting a group removes its shares, which can leave further projects and workflows without anybody, so the rules repeat until nothing changes.
- A resource without an owner keeps working for the users who hold a role on it; administrators can manage it and assign a new owner. The last enabled administrator cannot be deleted.
- Runs the user started are kept, without a starter.

The same rules are shown to the user in the app (`macworp_core::account_deletion::RULES`).

**Workers** authenticate with a static credential from the configuration. They are not database users, hold no shares and therefore see no projects or workflows through the API.

## Installation

### Desktop

Data is located in
| OS | Path |
| --- | --- |
| macOS | `~/Library/Application Support/de.ruhr-uni-bochum.CUBiMed.RUB.macworp_desktop` |
| Linux | `$XDG_DATA_HOME/macworp_desktop` (default `~/.local/share/macworp_desktop`) |
| Windows | `%APPDATA%\CUBiMed.RUB\macworp_desktop` (`C:\Users\<user>\AppData\Roaming\CUBiMed.RUB\macworp_desktop`) |

#### macOS
Comming soon

#### Linux
Comming soon

#### Windows
Comming soon

### Web
Comming soon

## Remote workflow cache
Workflows with a remote source (Git URL or nf-core pipeline) are downloaded by the backend into a cache and run from there, so engines need no network access and a run always uses the commit that was resolved when it was started. This matters on networks with limited access to repository hosts, so the cache can be filled ahead of time:

* **Saving a workflow** with a remote source starts a download in the background. The editor shows its progress and a warning if the host cannot be reached; the workflow is saved either way.
* **From the command line** an operator with access can fill or refresh the cache: `macworp_backend workflow-cache fetch https://github.com/owner/repo [tag|branch|commit] [-c config]`. The cache directory (`<storage_path>/workflows/remote_cache`) consists of plain directories and can be copied to a host without access. `workflow-cache list` shows the cached checkouts, `workflow-cache prune` removes unused old ones. The commands are safe to run while the backend is up: all writers lock `.macworp-cache.lock` in the cache directory.
* **Branches** are fetched again after `branch_cache_seconds` (default 3600) when needed. Set `branch_refresh_seconds` (default `0`, off) to let the backend update all workflows that follow a branch (or the default branch) in the background every N seconds. Tags and commit hashes are never refreshed.
* The workflow editor lists nf-core pipelines and releases from the public index (`https://nf-co.re/pipelines.json`), fetched by the backend. `nf_core_index_cache_seconds` (default 3600) sets how long a copy is served; if a refresh fails the last copy (also kept in `<storage>/workflows/nf_core_index.json`) is used. `nf_core_index_url` changes the address. Without any copy the editor falls back to free text.
* Checkouts unused for `checkout_max_age_days` (default 30, `0` keeps all) are deleted; checkouts of queued or running runs are kept.

Only plain `https://host/owner/repo` URLs are accepted. Private repositories are not supported (no credentials are stored); upload non-public workflows as local workflows instead.

## Local workflows
Workflows with a local source are uploaded as a zip archive in the workflow editor (a single top-level folder is removed). Every upload is a new immutable version below `<storage_path>/workflows/local/<workflow id>/`:

* A run uses the newest version at the moment it starts and keeps it until it has finished, so uploading again while runs are queued or running does not change them. Only runs started afterwards use the new files.
* Versions that are neither the newest nor used by a queued or running run are deleted after an upload, when the workflow is deleted and by an hourly cleanup.
* The desktop app reads the files from a folder you choose instead and does not copy them: changing that folder affects runs that are queued or running.

## Contribution
If you want to contribute to MAcWorP please check our [contribution document](CONTRIBUTING.md) first.

### Development
The ecosystem is build on top of [axum](https://crates.io/crates/axum) (backend) and [dioxus](https://crates.io/crates/dioxus) (web and desktop UI).
Code, shared between all components, can be found in the [core-crate](packages/core) shared UI code is maintained in the [ui_core-crate](packages/ui_core).  
MAcWorPs worker component lives in the [worker-crate](packages/worker) as a separate binary.

All styling is done via SASS which is transpiled and bundled via [manganis](https://crates.io/crates/manganis).


#### Requirements
* Rust: Follow [the install page](https://rust-lang.org/tools/install/) (use `rustup`)
  * x86_64 target: Depends on your operating system
  * WASM target: `rustup target add wasm32-unknown-unknown`
* Dioxus CLI: `curl -sSL https://dioxus.dev/install.sh | bash` or `cargo binstall dioxus-cli --force` or `cargo install dioxus-cli`
* On Linux or Mac, Procfile application: [Ultraman](https://github.com/yukihirop/ultraman) (Rust based) or use Procfile application like [foreman](https://github.com/ddollar/foreman) (Ruby basesd) or [honcho](https://github.com/nickstenning/honcho) (Python based)
* Container engine
  * Docker and docker-compose: Needed for PostgreSQL. Instruction depend on your operating system
  * Podman: Optional for testing and validation. Instruction depend on your operating system
  * WSL (Container): Optional for testing and validation. Instruction depend on your operating system
  
#### Running the desktop edition
`dx serve --package macworp_desktop --desktop --args='-vvv -a dev'`

To avoid collision between development and production `-a dev` adds a suffix `-dev` to the application directory, data is stored in:
| OS | Path |
| --- | --- |
| macOS | `~/Library/Application Support/de.ruhr-uni-bochum.CUBiMed.RUB.macworp_desktop-dev` |
| Linux | `$XDG_DATA_HOME/macworp_desktop-dev` (default `~/.local/share/macworp_desktop-dev`) |
| Windows | `%APPDATA%\CUBiMed.RUB\macworp_desktop-dev` (`C:\Users\<user>\AppData\Roaming\CUBiMed.RUB\macworp_desktop-dev`) |


#### Running the web edition

##### Linux / macOS
```
dev/setup.sh
ultraman start
```

This starts Postgres, the backend (:3001), a remote worker and the web UI (:5001). The worker talks to the backend over HTTP and needs podman or docker plus the `macworp-nextflow:latest` image (`docker build -f docker/engine.dockerfile --target nextflow -t macworp-nextflow:latest docker`; Snakemake: `--target snakemake -t macworp-snakemake:latest`). Create a user, open a project, start a Nextflow workflow and watch the run; stop the worker mid-run and the run is requeued after `worker_lease_seconds` (default 60, lower it in `dev/config.toml` to test faster).

##### Windows
```
# terminal 1
pwsh dev/setup.ps1
docker-compose up
```

```
# terminal 2
cargo run -p macworp_backend -- db-migrate dev/config.toml && cargo run -p macworp_backend -- serve dev/config.toml
```

```
# terminal 3
cargo run -p macworp_worker -- -vv --backend-url http://127.0.0.1:3001 --storage-path dev_storage --password-file dev/worker.password --name dev-worker run
```

```
# terminal 4
MACWORP_BASE_URL=http://127.0.0.1:3001 dx serve --package macworp_web --web --port 5001
```

##### Resetting all operating systems
Stop all the started processes from above and run `docker compose down --remove-orphans`


## For workflow developers
Follow the [workflow intregration guide](https://cubimedrub.github.io/macworp/latest/workflow_integration/). 

General recommendations
* Use public Docker container to manage process dependencies
* Do not use any experimental features in your workflows

We provided a simple genomics example workflow in [Nextflow](https://github.com/cubimedrub/macworp-genomics-demo-workflow-nextflow) and [Snakemake](https://github.com/cubimedrub/macworp-genomics-demo-workflow-snakemake) to show how it is done.
A well tested real work example is [McQuaC](https://github.com/mpc-bioinformatics/McQuaC), quality control workflow for mass spectrometry proteomic data.
