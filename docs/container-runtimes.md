# Container runtimes and engine images: decisions

Every workflow engine runs **inside an engine image**, started by the worker through the host's container runtime (Docker, Podman, later WSL containers). This page records what was tried, what works on which runtime, and why. It is the place for every runtime-related decision; ADRs ([0004](adr/0004-windows-strategy.md) for the overall strategy) link here for the details. When a spike changes a rule, update the table and the section below, and put the date and the host in the entry.

## What runs where

| Engine | Image | Docker | Podman | WSL containers (`wslc`) |
| --- | --- | --- | --- | --- |
| Nextflow | `macworp-nextflow` | yes | yes | no |
| Snakemake | `macworp-snakemake` | yes | **no** (see below) | no |
| Container (single image) | the workflow's own | yes | yes | yes |
| CWL (Toil) | `macworp-cwl` | yes | yes | no |

The code side of this table is `RuntimeKind::supports_engine` (`packages/worker/src/container_runtime/runtime.rs`); the start-up doctor (`macworp_worker doctor`) reports what a host can run and the UI greys out the rest.

## Engine images

**One multi-stage `docker/engine.dockerfile`, one target per engine**, not one image with all engines and not unrelated files.

- Shared `base` stage: container CLIs (`docker.io`, `podman-remote`), git, bash, procps, curl.
- Targets `nextflow`, `snakemake`, `cwl` add their engine only. Developers build them locally with
  the `Makefile` (`make engine-images-default`, `make engine-images`, `make engine-image ENGINE=cwl VERSION=8.2.0`,
  `make engine-images-list`; `RUNTIME=podman` and `REGISTRY=` override the CLI and the name prefix). The images get the
  names the worker pulls by default (`ghcr.io/cubimedrub/macworp-<engine>:<version>`, default version also `latest`),
  so no registry is needed; with `REGISTRY=` empty run the worker with `MACWORP_ENGINE_REGISTRY=` empty. By hand:
  `docker build -f docker/engine.dockerfile --target snakemake --build-arg SNAKEMAKE_VER=9.27.2 -t ghcr.io/cubimedrub/macworp-snakemake:9.27.2 docker`.
- Why not a single image: its size (JRE + two Python stacks + Apptainer), engine versions could not be pinned independently (a bump would rebuild and re-tag every engine), and the extra privileges one engine needs (below) would apply to all of them.
- Why not separate files: shared layers and one place to bump the base.
- Versions are `ARG`s (`NXF_VER`, `SNAKEMAKE_VER`, `PANOPTES_PLUGIN_VER`, `TOIL_VER`).

## Nextflow: sibling containers through the runtime socket

Decided in [ADR 0004](adr/0004-windows-strategy.md), spike 2026-10-22 on macOS.

- Nextflow starts the tool containers of its processes itself. The engine container gets the runtime socket (`RuntimeInfo::engine_access_args`) and a docker CLI, so the tool containers are **siblings** on the host runtime.
- The project directory is bind-mounted at the **identical path** inside the engine container, so mounts the engine creates resolve on the host.
- Works on Docker Desktop (`/var/run/docker.sock`) and on a rootless podman machine (`remoteSocket`, `--security-opt label=disable`).
- Nested podman inside the engine container was **rejected**: `mount proc to proc: Operation not permitted` with default settings, needs extra privileges, and every run re-pulls images.
- Cost: the socket gives the engine control of the host runtime. Hardening is P6.

### Nextflow: executor and container engine are fixed by MAcWorP (2026-11-20, macOS)

Workflows pick software deployment through their own configuration and `-profile` (`docker`, `conda`, `singularity`, `slurm`, ...). Those choices cannot work in the engine container, so the worker overrides them on every run.

- **What is passed:** `-c <scratch>/deployment.config` (`DEPLOYMENT_CONFIG_CONTENT` in `executors/nextflow.rs`, next to the weblog config of Nextflow 26): `process.executor = 'local'`, a catch-all `withName: '.*'` selector with the same executor (see "Per-process executors" below), `docker.enabled = true`, and `podman`, `singularity`, `apptainer`, `charliecloud`, `shifter`, `conda`, `mamba`, `spack` `.enabled = false`.
- **Why `docker.enabled` on Podman too:** the engine image has the `docker` client (and `podman-remote`, only for Toil). Nextflow starts tool containers by running that client against `DOCKER_HOST=unix:///run/container.sock`. Podman's socket speaks the Docker-compatible API, so the same client works on both runtimes, and `podman.enabled = true` would need a `podman` binary that is only the remote client and was not tried. So the container engine in the config is always `docker`, regardless of which runtime the worker uses.
- **Why `-c` and not a CLI option:** the docs list `-with-docker` / `-with-podman` for `nextflow run` but no executor option, and `-process.executor` is rejected as an unknown option. A `-c` file is merged on top of the workflow's config and **takes precedence over `-profile`**. Checked on Nextflow 24.10.9, 25.10.9 and 26.10.6 with `nextflow config` and real runs, profiles `slurm,conda,singularity` (without the file the run used executor `slurm` and failed, with it `local`). `-C` is not used: it would drop the workflow's own configuration.
- **Verified end to end:** a workflow whose process names `container 'debian:12-slim'`, run with profiles `slurm,conda,singularity` and the config above, started the tool container as a sibling and succeeded on Docker Desktop (Docker 29.9) and on a rootless podman machine (Podman 6.1.4), engine image 26.10.6, with the `engine_access_args` of each runtime. The desktop e2e `runs_nextflow_workflow_on_all_versions` (all three versions, Docker) and `runs_nextflow_workflow_with_all_parameter_types` (Podman) pass; their fixture has no container task, the manual run above covers that. Note the tool image needs `bash` (an `alpine` image fails with exit 127, unrelated to the runtime).
- **Per-process executors:** `process.executor` alone is **not enough**. A workflow can set the executor per process with `process { withName: 'x' { executor = 'slurm' } }`, with `withLabel:`, inside a profile, or as an `executor 'slurm'` directive in the script, and all of these beat it (checked: Nextflow 24.10.9 and 26.10.6 ran such processes with `slurm`). The file therefore also has a catch-all `process { withName: '.*' { executor = 'local' } }`, which beats all of them (checked on 24.10.9, 25.10.9 and 26.10.6 with a `withName` block, a `withLabel` block, a profile selector and a script directive: all ran `local`).
- **Not overridden:** other options a profile or process sets (`process.container`, `containerOptions`, `docker.runOptions`, `docker.registry`). Not verified: Linux hosts, Docker rootless, wslc (Nextflow is unavailable there).
- **Engine parameter `profile`:** stays allowed for workflow-specific profiles (`test`, `small`); its editor description says that MAcWorP overrides the executor and deployment (`engine_parameters.rs`). Other deployment options (Snakemake's method, conda, apptainer) are not engine parameters at all.

## Snakemake: Apptainer inside the engine container

Spike 2026-11-16 on macOS (Apple silicon, Docker Desktop 29, Podman 6.1 machine). Snakemake 9.11.

**Snakemake does not use the runtime socket.** Rules with a `container:` are run by Apptainer, which pulls the image itself and runs it in its own namespaces. So the "Docker only because of the socket" assumption from the plan was wrong: the real requirement is that Apptainer can start inside the engine container. Findings, in the order they were hit:

| Attempt (all inside `macworp-snakemake`) | Result |
| --- | --- |
| default container, root | `Failed to create user namespace` |
| `--cap-add SYS_ADMIN`, seccomp/apparmor unconfined, root | user namespace ok, then `could not open /dev/loop0: operation not permitted` (no loop device in a container) |
| same + `--device /dev/fuse` + squashfuse, root | Apptainer skips its FUSE driver as root and still wants the loop device |
| `--userns`, root or not, SIF mounted with squashfuse | `exec /bin/sh failed: invalid argument` |
| `--privileged` | works, but is the full-privilege sandbox we want to avoid |
| **non-root user, squashfuse removed, `seccomp=unconfined` + `systempaths=unconfined`** | **works** (Apptainer unpacks the image into a temporary sandbox with `unsquashfs`) |
| the same as root | `unsquashfs` extraction fails |
| the same on Podman (`systempaths=unconfined` or `unmask=ALL`) | `can't mount proc filesystem to /proc: permission denied` |

**Decision.** The Snakemake executor starts the engine with:

- `--security-opt seccomp=unconfined --security-opt systempaths=unconfined`: Apptainer creates user namespaces and mounts `/proc` in them, which the default seccomp profile and the masked `/proc` paths forbid. No capabilities, no devices, no socket.
- `--user <uid>:<gid>` of the project directory's owner (Apptainer cannot unpack as root; files the workflow writes stay owned by that user). The image entrypoint adds a `/etc/passwd` entry for that uid, without it Apptainer stops with `unknown userid`.
- `-e HOME=/tmp` for Apptainer's cache.
- The image has **no squashfuse** (removed in the Dockerfile) and `squashfs-tools` instead, so there is no need for `/dev/fuse` or a loop device.
- `--software-deployment-method apptainer` and `--apptainer-args "-B <dir>:<dir>:ro ..."` for the workflow and reference directories. Both are skipped when an engine parameter already picks a deployment method (`use-conda`, `sdm`, ...).

Consequences and open points:

- **Docker: macOS yes, Ubuntu Linux no (as configured).** See "Linux host spike" below. **Podman: Linux yes, macOS machine no.** `RuntimeKind::supports_engine` therefore treats Podman as Snakemake-capable on Linux hosts only (`cfg!(target_os = "linux")`), Docker everywhere. The worker picks the first runtime supporting the engine (podman first), so on a Linux host with both installed Snakemake runs on podman.
- **macOS verification (2026-11-16, Docker Desktop 29.9, Podman 6.1 machine).** Docker: the Snakemake e2e passes with the split executor (`DOCKER_APPTAINER_RUN_ARGS`). Podman machine: still fails with the Podman options (`--userns=keep-id --user <uid>:<gid> --security-opt seccomp=unconfined --security-opt unmask=ALL`, gate bypassed temporarily): `mount proc->/proc error: ... can't mount proc filesystem to /proc: permission denied`. So `keep-id` does not help in the machine VM, the `cfg!(target_os = "linux")` limit stays.
- The rule's image needs `bash` (Snakemake runs shell rules with `bash -c`), `alpine` does not work.
- A weaker sandbox than other engines (seccomp and `/proc` masking off for the engine container only). Revisit in P6 with a custom seccomp profile (on Podman both `seccomp=unconfined` and `unmask=ALL` were each required, see below).
- Images from a registry are pulled by Apptainer, not by the runtime: they do not appear in `docker images`, and the P6 image GC does not see them. They are cached in one shared directory outside the projects (2026-11-18, macOS, Docker Desktop): `<storage>/cache/apptainer` on a standalone worker (`core::paths::APPTAINER_CACHE_DIRECTORY`), a folder in the desktop settings (default `<app data>/cache/apptainer`). The executor passes `--apptainer-prefix <dir>` and mounts it read-write at the same path into the engine container; both are skipped when an engine parameter sets `apptainer-prefix`/`singularity-prefix` or a deployment method. `<project>/.snakemake/singularity` stays empty. Not checked: concurrent pulls of one image, eviction (the P6 GC must cover this directory), workers with different uids on one shared storage (the cache is owned by the first writer).
- Apptainer: the GitHub release `.deb` is amd64 only; the image uses `ppa:apptainer/ppa` which also serves arm64 (Apple silicon).

### Linux host spike (2026-11-16)

Ubuntu, kernel `apparmor_restrict_unprivileged_userns=1`, Docker 29.9.3, rootless Podman 5.7.0, Apptainer 1.4.5, same `macworp-snakemake` image. Test: a rule with `container: docker://debian:stable-slim`, run with the executor's arguments.

| Runtime and options | Result |
| --- | --- |
| Docker, `seccomp=unconfined` + `systempaths=unconfined`, `--user` | `Failed to set mount propagation: Permission denied` (default `docker-default` AppArmor profile) |
| the same + `apparmor=unconfined` | `unshare: write failed /proc/self/uid_map: Operation not permitted` (the host restriction hits unconfined processes) |
| the same + `--cap-add SYS_ADMIN` | no change (the user is not root) |
| Podman, `seccomp=unconfined` + `unmask=ALL`, root in the user namespace | image pulled, `unsquashfs`: `failed to change uid and gids on .../etc/shadow` |
| **Podman, `--userns=keep-id --user <uid>:<gid>`, `seccomp=unconfined` + `unmask=ALL`** | **works** (rule output written, owned by the user) |
| the same without `seccomp=unconfined`, or without `unmask=ALL` | fails |

Docker on such a host needs the AppArmor profile `docker/apparmor-macworp-snakemake` (docker-default plus `userns`, `mount`, `remount`, `pivot_root`; load with `sudo apparmor_parser -r -W docker/apparmor-macworp-snakemake`) or `kernel.apparmor_restrict_unprivileged_userns=0`. The worker applies the profile when `MACWORP_SNAKEMAKE_APPARMOR_PROFILE=macworp-snakemake` is set (Docker only); it cannot detect it, loaded profiles are not readable without root and Docker refuses a container whose profile is not loaded. **E2E (same host, 2026-11-16):** `runs_snakemake_workflow_with_all_parameter_types` passes on rootless Podman, and on Docker with `MACWORP_SNAKEMAKE_APPARMOR_PROFILE=macworp-snakemake` (all four e2e tests pass); without the variable Docker fails with `Failed to set mount propagation`. The Podman Nextflow e2e was skipped (no `macworp-nextflow` image in podman: its build cannot resolve the short name `eclipse-temurin` without a configured registry). Not verified: rootful Podman; other distributions.

`doctor` warns (check `apparmor`) when Docker is the Snakemake runtime, `/proc/sys/kernel/apparmor_restrict_unprivileged_userns` is `1` and the variable is unset. Install on the worker host: `sudo apparmor_parser -r -W docker/apparmor-macworp-snakemake` (reload after edits, remove with `-R`), then start the worker with `MACWORP_SNAKEMAKE_APPARMOR_PROFILE=macworp-snakemake`. When the variable is set, `doctor` verifies the profile by starting a throwaway container with it (`docker run --rm --pull never --security-opt apparmor=<profile> --entrypoint true <engine image>`): a profile that is not loaded gives a warning (verified 2026-11-17 on the Ubuntu host: unset, loaded and unknown profile). Without the engine image locally the profile is reported as not verified. The profile does not survive a reboot unless it is copied to `/etc/apparmor.d/`.

## Snakemake: progress reporting

Snakemake 9 removed `--wms-monitor`. Progress comes from the
[`snakemake-logger-plugin-panoptes`](https://github.com/panoptes-organization/snakemake-logger-plugin-panoptes) (installed in the engine image, pinned) via `--logger panoptes --logger-panoptes-address <worker log proxy>/snakemake`. The log proxy (`packages/worker/src/log_proxy`) mimics the Panoptes API the plugin talks to. Verified 2026-11-16: task counts arrive (`tasks_total`/`tasks_done`) in the end-to-end run.

## CWL: Toil with sibling containers through the runtime socket

Spike and implementation 2026-11-17, macOS (Docker Desktop 29.9, podman 6.1 machine), Toil 9.6.0.

- Toil (`toil-cwl-runner`) starts `DockerRequirement` images through the runtime socket exactly like Nextflow: same `engine_access_args`, engine container as root, no Apptainer, no extra security options. Works on Docker and on the rootless podman machine (`--security-opt label=disable`). `--podman` is not needed, the docker CLI talks to the podman socket.
- Toil pre-pulls the images with `cwl-docker-extract`, which lives in the venv: the image puts `/opt/toil/bin` on `PATH` (without it the run fails with `No such file or directory: 'cwl-docker-extract'`).
- Toil mounts files from its working directory into the tool containers, so the working directory must exist on the host: the executor uses `<project>/.toil` (`--workDir`, `--jobStore <project>/.toil/jobstore`, created by the executor, Toil does not create the parent) and `--outdir <project>`. Engine parameters `jobStore` / `workDir` replace the defaults. The empty `.toil` directory stays in the project.
- Workflow parameters are the CWL input object (JSON file, `core::render::render_parameter_file`). `Path` values become `File` / `Directory` objects with absolute paths (relative paths would be read relative to the input file); a directory on the host is a `Directory`, anything else a `File`. Input names are flat (no dotted nesting).
- Progress: Toil has no event hook. Its leader log (`Issued job ...`, `CWL step complete`) could be parsed; not done, a CWL run shows indeterminate progress.
- Only local sources; remote CWL repositories are not cloned by the worker yet.

### CWL: memory and resume (2026-11-19, macOS, Docker Desktop and podman machine, Toil 9.6.0)

- **Memory:** Toil reserves 2 GiB per job unless the tool declares `ramMin`, and refuses to schedule a job above the memory it detects ("requesting 2147483648 bytes of memory, more than the maximum"). The podman machine has 1.9 GiB, so every run failed there, also on restart. The executor passes `--defaultMemory 512M` (a scheduling reservation, not a limit) unless an engine parameter `defaultMemory` is set. Reproduced on Docker with `--maxMemory 1900M`.
- **`--restart` is not a resume for corrected input:** it continues the stored job graph with the stored workflow, inputs and resource requests. After a failure caused by an empty value, restarting with a corrected input object reran the job with the old value. It only helps after a crash or cancel. MAcWorP does not use it.
- **Resume = fresh run with a step cache:** `--cachedir <storage>/cache/cwl` (`core::paths::CWL_CACHE_DIRECTORY`, desktop: `<app data>/cache/cwl`, mounted read-write at the same path) caches step outputs by input; a fresh run with corrected values skipped the unchanged step (checked with a two-step workflow: the first step's timestamp stayed identical) and reran the failed one (a failed step is cached with status `permanentFail` but not reused). Because the job store is not restarted, the executor empties `<project>/.toil` before each run (an existing job store makes a fresh run fail).
- **Temporary folders:** with `--cachedir` cwltool no longer uses Toil's file store for step folders and falls back to the engine container's `/tmp`, which the sibling tool containers cannot mount (`bind source path does not exist: /host_mnt/private/tmp/...`, docker error 125). The executor passes `--tmp-outdir-prefix` and `--tmpdir-prefix` `<project>/.toil/tmp/`.
- **Open:** the cache grows (P6 GC must know `cache/cwl`); outputs are shared between all projects and users of one storage (the key hashes the tool, input values and input file content, so equal runs share results, review before multi-tenant use); several workers writing one entry concurrently, uid mismatch between workers; not run on Linux, podman or in `dx`.

## Workflow parameters: Nextflow and Snakemake read a file

Workflow parameters are written as JSON (valid YAML) to `parameters.json` in a scratch directory of the run (`<temp dir>/macworp-run-<id>-*`, readable by everyone, removed by the worker when the run ended) and passed with `-params-file` (Nextflow) or `--configfile` (Snakemake); the directory is mounted read-only like the workflow files (`executors/parameter_file.rs`, `core::render::render_parameter_file`). Dotted parameter names become nested objects, numbers and true/false choices keep their type, everything else is a string. The Container engine still gets `--name value` arguments. The backend renders the file content at run start too, so invalid names or values fail there and not in the worker.

Command line parameters are not used because the engines parse each value again. Snakemake `--config` (checked with 9.11): a line break becomes a space, `say "hi"` loses its closing quote, `a: b` and `{a: 1}` become dictionaries, `[1,2]` a list, `"7"` an integer; quoting does not help. Nextflow `--name value` types values too (a text `7` becomes a number). Verified 2026-11-16 on Docker Desktop (macOS): the end-to-end runs of both engines keep the line break. The scratch directory must be visible to the runtime; podman machine and WSL are unverified.

Engine images (2026-11-20): one image per engine **and version**, listed in `docker/engine-versions.yml` (target, build argument, default, versions; read by `core::workflow_configuration::engine::Engine::catalog` and by CI). `.github/workflows/engine-images.yml` builds each (amd64 + arm64) and pushes `ghcr.io/cubimedrub/macworp-<engine>:<version>`, the default version also as `latest`; pull requests only build. Adding a version = add it to the JSON. The worker runs `<registry>/macworp-<engine>:<default version>` unless the workflow pins a version (`engine_version`, a dropdown in the workflow editor; the backend rejects versions outside the catalog, 400), then the tag is swapped (`executors::engine_image`). `MACWORP_ENGINE_REGISTRY` replaces the registry (empty = no registry, finds locally built `macworp-nextflow:25.04.6`); `MACWORP_NEXTFLOW_IMAGE` / `MACWORP_SNAKEMAKE_IMAGE` / `MACWORP_CWL_IMAGE` replace the default image as a whole (a pin still swaps its tag). Ignored by the `Container` engine. The catalog is YAML (`docker/engine-versions.yml`, parsed by `serde_norway`; CI reads it with `yq`) and lists the latest release of each of the last three majors: Nextflow 26.10.6 / 25.10.9 / 24.10.9, Snakemake 9.27.2 / 8.30.0 / 7.32.4, Toil 9.6.0 / 8.2.0 / 7.0.0. Per-major differences (all verified by the per-version e2e tests on 2026-11-20: Docker on macOS, and Docker on a Fedora 45 ARM VM with SELinux enforcing, 11 of 11 pass): **Nextflow 26** removed `-with-weblog`; the image has the `nf-weblog` plugin installed (`NF_WEBLOG_VER`) and the worker writes a `weblog.config` (plugin + `weblog.url`) into the run's scratch directory and passes `-c` (`uses_weblog_plugin`); the `for` statement of the test workflow had to become `.each` because 26 enforces the strict syntax. **Snakemake 9** reports progress with the Panoptes logger plugin, **8 and 7** with `--wms-monitor` (no plugin in the image; the receiver accepts the form body of 8/9 and the content-type-less JSON body of 7); **7** uses `--use-singularity` / `--singularity-prefix` / `--singularity-args` (`singularity` is Apptainer's alias), needs `pulp<2.8`, and its Snakefile parser mangles f-strings on Python 3.12, so its image uses Python 3.11 from the deadsnakes PPA (`executors::snakemake::Generation`). **Toil 7** has no `--cachedir`, so it runs without the shared step cache (a resumed run recomputes every step, `CwlExecutor::supports_step_cache`). gcc/python3-dev are installed and purged in the Python stages because psutil/datrie have no wheel on arm64. Run the per-version tests with `make engine-images` and `cargo test -p macworp_desktop end_to_end_test::runs_ -- --ignored --nocapture` (`..._on_all_versions`). **Unverified:** the CI workflow has not run (ghcr packages must be public for anonymous pulls).

## End-to-end test coverage

`cargo test -p macworp_desktop end_to_end_test -- --ignored` runs one scenario per engine (all parameter types, fixed and dynamic) on the selected runtime (`MACWORP_CONTAINER_RUNTIME`). Needs the engine images built as above. `runs_cwl_workflow` (text, number, file through Toil) is a smaller scenario, not the all-parameter-types one. Passing on Docker (macOS) for Nextflow, Snakemake, CWL and Container, and on Podman for Nextflow, CWL and Container; Podman is expected to reject Snakemake runs.

## Engine user (2026-11-17, macOS: Docker Desktop and podman machine)

Nextflow, CWL (Toil) and Snakemake engine containers run as the worker's `uid:gid` (`--user`, `HOME=/tmp`), so what they write below the shared storage belongs to the user that owns the storage and the worker can manage it. The `Container` engine keeps the user of the workflow author's image.

- **Docker**: the runtime socket is only usable with its group, `--group-add <gid>`. On a Linux host that is the group of the socket file. Docker Desktop mounts the socket as `root:root 660`, group `0` works (verified, without it `permission denied while trying to connect to the docker API`).
- **Podman (rootless)**: `--userns=keep-id` maps the user to itself, the socket is then owned by that user inside the container (verified on the podman machine, `--group-add keep-groups` is not supported in remote mode).
- **Nextflow image**: `/opt/nextflow` is world readable and writable (the framework jar was `0600 root`, so a non-root engine failed with `Unable to access jarfile`).
- Not verified: file ownership on a Linux host, Docker with the group path on Linux, Nextflow task containers (started through the socket, root unless the workflow sets `docker.runOptions`).

## Cleanup after a successful run (2026-11-18, not run against a real engine)

Per-workflow switch `cleanup_on_success` (workflow editor, Nextflow and CWL only). Only after a
`Succeeded` run, never after failure or cancel, so those stay resumable.

- Nextflow: a second `run --rm` of the engine image with the project mount runs
  `nextflow clean -f` (removes `work/` of the last run; `.nextflow/` and `.nextflow.log` stay)
  **as root**, because task containers (started through the socket, possibly root-only images)
  can leave root-owned files on Linux hosts. No `-u` is forced onto task containers. A final
  `chown -R <uid>:<gid> .nextflow .nextflow.log` hands the bookkeeping back to the worker's
  user. Not verified on a Linux host.
- CWL: Toil deletes its job store on success itself; the worker removes the empty `.toil/`
  (skipped when `jobStore` or `workDir` is an engine parameter).
- Snakemake, Container: nothing (`.snakemake/` holds the knowledge for incremental reruns).
- A cleanup failure is logged by the worker, the run stays `Succeeded`.

## SELinux (2026-11-18, Fedora 45 ARM VM, Docker 29)
- With SELinux `Enforcing` every bind mount of an engine container (project, workflow, parameter file, reference data, image cache) fails with "Permission denied", for all engines including `Container` (control run of the e2e). `Permissive` passes.
- `containerized_spec` adds `--security-opt label=disable` when `/sys/fs/selinux/enforce` is `1` (`executors::selinux_enforcing` / `selinux_args`). Only the engine container loses its label confinement, the host policy and other containers are unchanged. `doctor` shows a `selinux` warning with this.
- Not `:z`/`:Z`: they relabel host directories for good, including folders the desktop user picks and storage without labels (NFS). Not a custom policy type for now: mounted paths change per run and the engine needs the runtime socket (P6).
- **Containers the engine starts itself** (CWL tool containers, Nextflow tasks, Snakemake without Apptainer) keep the daemon's confinement and cannot use bind mounts of unlabelled files. Fix, narrow on purpose: label only the folders MAcWorP owns `container_file_t` (`worker::selinux::DirectoryLabel`); files created below inherit the type. Standalone: the administrator labels the storage root (`sudo semanage fcontext -a -t container_file_t '<storage>(/.*)?' && sudo restorecon -R <storage>`), `macworp_worker doctor` checks it (`selinux-labels`, warns with that command). Desktop: `Settings::load` runs `chcon -R -t container_file_t` on the default reference, workflow cache and image cache folders; folders the user picked are never relabelled automatically, the settings page offers a switch per folder (`Settings::set_container_access`, shown only when SELinux enforces) that applies `chcon -R -t container_file_t` or gives it the label of its parent folder (`chcon -R --reference`; `restorecon` does nothing for folders without a policy default). Never disable SELinux daemon-wide.
- Verified 2026-11-19 on the Fedora VM, Docker, `Enforcing`: e2e Container, Nextflow, Snakemake and CWL all pass (the e2e labels its own temp root); without the labelled root CWL tool containers fail (`cannot create result.txt`). Not checked: Nextflow with container tasks (fixture has none), Podman on Fedora, a custom policy type (P6).

## Podman on Linux (Fedora 45, SELinux enforcing, 2026-11-20)

Host: Fedora 45 ARM VM, Podman 6.2 rootless, SELinux `Enforcing`. All engine images built there with `make engine-images RUNTIME=podman`; per-version e2e run with `MACWORP_CONTAINER_RUNTIME=podman`.

- **Works:** Nextflow 26/25/24, CWL (Toil 9/8/7) and `Container`.
- **Socket:** the engine reaches Podman through its API socket, which is not running by default on Fedora: `systemctl --user enable --now podman.socket` (without it every engine run fails with `statfs /run/user/1000/podman/podman.sock: no such file or directory`, exit 125). The doctor does not check this yet.
- **CWL needs Toil's `--podman` mode:** cwltool then starts the tool containers with `--userns=keep-id`. Without it the tool's user (`--user 1000:1000`) is a subordinate id that does not own the mounted project directory (`cannot create cwl_result.txt: Permission denied`). The executor adds `--podman` for the Podman runtime (`CwlExecutor::with_runtime_kind`), the engine access args set `CONTAINER_HOST` for the `podman-remote` client and the `cwl` image links it as `podman`. Also verified on the macOS Podman machine (default versions).
- **Snakemake does not work on Podman with SELinux enforcing:** Apptainer cannot unpack its images (`write_xattr: could not write xattr security.selinux for file /image/root/.exec because you're not superuser!`, `root filesystem extraction failed`), the SIF files carry the `security.selinux` attributes of the labelled container filesystem. Tried without success: `--tmpfs /tmp`, wrapping the bundled `unsquashfs` (it runs inside Apptainer's own mount namespace, a script wrapper breaks it and a binary cannot reach the real one). Docker on the same host works. `RuntimeKind::podman_runs_snakemake()` is therefore `linux && !selinux_enforcing()`, so Snakemake is greyed out / rejected with 409 on such a host (and `Auto` picks Docker for it when both are installed). Not tried: disabling SELinux (host-wide, ruled out), a policy module.
- **Short image names:** Podman has no default unqualified-search registry on Fedora, so `engine.dockerfile` names `docker.io/library/eclipse-temurin` in its `COPY --from`.

## Non-public images (spike 2026-11-23, macOS: Docker Desktop 29.9 and podman 6.1.4 machine)

Decision proposal in [ADR 0007](adr/0007-non-public-container-images.md). Verified by hand with the `macworp-nextflow:26.10.6` engine image and the socket arguments of `engine_access_args`: an image built on the host (`docker build` / `podman build`) is used by Nextflow's tool containers; a private registry pull from the engine container fails without credentials (`no basic auth credentials`) and works with a credential-store-free `config.json` via `DOCKER_CONFIG`; an image pulled on the host beforehand needs no credentials; Apptainer in the Snakemake image authenticates with `APPTAINER_DOCKER_USERNAME` / `APPTAINER_DOCKER_PASSWORD`. Not checked: Linux, rootless Docker, wslc, Snakemake/CWL with a locally built image.

### Locally built images per engine (2026-11-23, macOS, same spike)

- **Toil 9.6.0:** `cwl-docker-extract` (pre-pull) fails for a local image and aborts the run; `toil-cwl-runner --no-prepull` runs it (Docker, and Podman with `--podman`).
- **Snakemake 7.32.4 / 8.30.0 / 9.27.2:** Apptainer does not see the runtime's images. A SIF placed at `<apptainer-prefix>/<md5 of "docker://name:tag">.simg` is used without pulling (the md5 naming is Snakemake internal); `docker-daemon:name:tag` (socket mounted) and `docker-archive:<tar>` URLs also work but need a changed Snakefile. Not checked: Podman on Linux, building the SIF from an archive in a helper container.
