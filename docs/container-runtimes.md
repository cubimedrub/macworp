# Container runtimes and engine images: decisions

Every workflow engine runs **inside an engine image**, started by the worker through the host's container runtime (Docker, Podman, later WSL containers). This page records what was tried, what works on which runtime, and why. It is the place for every runtime-related decision; ADRs ([0004](adr/0004-windows-strategy.md) for the overall strategy) link here for the details. When a spike changes a rule, update the table and the section below, and put the date and the host in the entry.

## What runs where

| Engine | Image | Docker | Podman | WSL containers (`wslc`) |
| --- | --- | --- | --- | --- |
| Nextflow | `macworp-nextflow` | yes | yes | no |
| Snakemake | `macworp-snakemake` | yes | **no** (see below) | no |
| Container (single image) | the workflow's own | yes | yes | yes |
| CWL (Toil) | `macworp-cwl` | not built yet (P5) | | |

The code side of this table is `RuntimeKind::supports_engine` (`packages/worker/src/container_runtime/runtime.rs`); the start-up doctor (`macworp_worker doctor`) reports what a host can run and the UI greys out the rest.

## Engine images

**One multi-stage `docker/engine.dockerfile`, one target per engine**, not one image with all engines and not unrelated files.

- Shared `base` stage: container CLIs (`docker.io`, `podman-remote`), git, bash, procps, curl.
- Targets `nextflow`, `snakemake` (later `cwl`) add their engine only. Build with
  `docker build -f docker/engine.dockerfile --target snakemake -t macworp-snakemake:latest docker`.
- Why not a single image: its size (JRE + two Python stacks + Apptainer), engine versions could not be pinned independently (a bump would rebuild and re-tag every engine), and the extra privileges one engine needs (below) would apply to all of them.
- Why not separate files: shared layers and one place to bump the base.
- Versions are `ARG`s (`NXF_VER`, `SNAKEMAKE_VER`, `PANOPTES_PLUGIN_VER`).

## Nextflow: sibling containers through the runtime socket

Decided in [ADR 0004](adr/0004-windows-strategy.md), spike 2026-10-22 on macOS.

- Nextflow starts the tool containers of its processes itself. The engine container gets the runtime socket (`RuntimeInfo::engine_access_args`) and a docker CLI, so the tool containers are **siblings** on the host runtime.
- The project directory is bind-mounted at the **identical path** inside the engine container, so mounts the engine creates resolve on the host.
- Works on Docker Desktop (`/var/run/docker.sock`) and on a rootless podman machine (`remoteSocket`, `--security-opt label=disable`).
- Nested podman inside the engine container was **rejected**: `mount proc to proc: Operation not permitted` with default settings, needs extra privileges, and every run re-pulls images.
- Cost: the socket gives the engine control of the host runtime. Hardening is P6.

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
- Images from a registry are pulled by Apptainer, not by the runtime: they do not appear in `docker images`, and the P6 image GC does not see them. They are cached under `<project>/.snakemake/singularity`.
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

## Snakemake: progress reporting

Snakemake 9 removed `--wms-monitor`. Progress comes from the
[`snakemake-logger-plugin-panoptes`](https://github.com/panoptes-organization/snakemake-logger-plugin-panoptes) (installed in the engine image, pinned) via `--logger panoptes --logger-panoptes-address <worker log proxy>/snakemake`. The log proxy (`packages/worker/src/log_proxy`) mimics the Panoptes API the plugin talks to. Verified 2026-11-16: task counts arrive (`tasks_total`/`tasks_done`) in the end-to-end run.

## Workflow parameters: Nextflow and Snakemake read a file

Workflow parameters are written as JSON (valid YAML) to `parameters.json` in a scratch directory of the run (`<temp dir>/macworp-run-<id>-*`, readable by everyone, removed by the worker when the run ended) and passed with `-params-file` (Nextflow) or `--configfile` (Snakemake); the directory is mounted read-only like the workflow files (`executors/parameter_file.rs`, `core::render::render_parameter_file`). Dotted parameter names become nested objects, numbers and true/false choices keep their type, everything else is a string. The Container engine still gets `--name value` arguments. The backend renders the file content at run start too, so invalid names or values fail there and not in the worker.

Command line parameters are not used because the engines parse each value again. Snakemake `--config` (checked with 9.11): a line break becomes a space, `say "hi"` loses its closing quote, `a: b` and `{a: 1}` become dictionaries, `[1,2]` a list, `"7"` an integer; quoting does not help. Nextflow `--name value` types values too (a text `7` becomes a number). Verified 2026-11-16 on Docker Desktop (macOS): the end-to-end runs of both engines keep the line break. The scratch directory must be visible to the runtime; podman machine and WSL are unverified.

Engine versions: the engine image is `macworp-nextflow:latest` / `macworp-snakemake:latest`; `MACWORP_NEXTFLOW_IMAGE` / `MACWORP_SNAKEMAKE_IMAGE` replace it (for example an older version built from `docker/engine.dockerfile` with another `NXF_VER` / `SNAKEMAKE_VER`). The end-to-end test honours them too.

## End-to-end test coverage

`cargo test -p macworp_desktop end_to_end_test -- --ignored` runs one scenario per engine (all parameter types, fixed and dynamic) on the selected runtime (`MACWORP_CONTAINER_RUNTIME`). Needs the engine images built as above. Passing on Docker (macOS) for Nextflow, Snakemake and Container, and on Podman for Nextflow and Container; Podman is expected to reject Snakemake runs.
