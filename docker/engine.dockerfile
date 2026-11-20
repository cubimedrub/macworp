# Prebuilt by CI for every version in docker/engine-versions.yml (.github/workflows/engine-images.yml) as
# ghcr.io/cubimedrub/macworp-<engine>:<version>; a workflow pins one of them.
#
# Engine images: one workflow engine per image, so versions and privileges stay independent.
# The engine starts sibling tool containers through a mounted runtime socket, see
# docs/adr/0004-windows-strategy.md.
#
# Developers build them with the Makefile (names and versions come from engine-versions.yml):
#
#   make engine-images-default      default version of each engine
#   make engine-images              every version
#   make engine-image ENGINE=nextflow VERSION=24.10.9
#
# By hand, one target (build argument per engine: NXF_VER, SNAKEMAKE_VER, TOIL_VER):
#
#   docker build -f docker/engine.dockerfile --target nextflow --build-arg NXF_VER=26.10.6 \
#     -t ghcr.io/cubimedrub/macworp-nextflow:26.10.6 docker

# Shared by all engines: container CLIs and basic tools
FROM ubuntu:24.04 AS base

RUN apt-get update \
    && apt-get install -y --no-install-recommends bash curl ca-certificates procps git docker.io podman-remote \
    && rm -rf /var/lib/apt/lists/*

# Nextflow (needs a JRE)
FROM base AS nextflow

ARG NXF_VER=26.10.6
ARG NF_WEBLOG_VER=1.2.0
ENV NXF_VER=${NXF_VER} NXF_HOME=/opt/nextflow NXF_ANSI_LOG=false NXF_OPTS="-Xmx1g"

COPY --from=eclipse-temurin:21-jre /opt/java/openjdk /opt/java/openjdk
ENV JAVA_HOME=/opt/java/openjdk PATH=/opt/java/openjdk/bin:$PATH

RUN curl -fsSL https://get.nextflow.io | bash \
    && mv nextflow /usr/local/bin/nextflow \
    && chmod 755 /usr/local/bin/nextflow \
    && nextflow -version \
    # Nextflow 26 removed `-with-weblog`, progress goes through this plugin (the worker enables it
    # with a generated config); installed here so no run has to download it
    && if [ "${NXF_VER%%.*}" -ge 26 ]; then nextflow plugin install "nf-weblog@${NF_WEBLOG_VER}"; fi \
    # The engine runs as the worker's user, not root: it must read the framework jar and may
    # write plugins and caches below NXF_HOME (the directory only lives as long as the container)
    && chmod -R a+rwX /opt/nextflow

# Snakemake (Python venv of its own) with Apptainer, which Snakemake uses to run `container:` images
FROM base AS snakemake

ARG SNAKEMAKE_VER=9.27.2
# Reports progress to the worker (Snakemake 9 removed --wms-monitor)
ARG PANOPTES_PLUGIN_VER=0.3.0
# Apptainer from its PPA: the release .deb files exist for amd64 only, the PPA also serves arm64
# (Apple silicon). Inside a container there is no loop device and usually no /dev/fuse, so the
# FUSE helper (bundled squashfuse) is removed: Apptainer then unpacks images with unsquashfs into
# a temporary sandbox. It must run as a non-root user for that, see docs/container-runtimes.md.
# gcc and python3-dev only build wheels that have none for the platform (psutil, datrie on arm64);
# they are removed again. Per major version: 9 reports progress through the Panoptes logger plugin
# (needs Snakemake 9, `--wms-monitor` was removed there), 8 and 7 use `--wms-monitor` and need no
# plugin. 7 breaks with PuLP 2.8 or newer, and its Snakefile parser mangles f-strings on Python
# 3.12 (it predates PEP 701), so it gets Python 3.11 from the deadsnakes PPA. Snakemake 7 starts
# images with the `singularity` command, which Apptainer provides.
RUN apt-get update \
    && apt-get install -y --no-install-recommends python3 python3-venv python3-pip software-properties-common gcc python3-dev \
    && add-apt-repository -y ppa:apptainer/ppa \
    && python=python3 \
    && case "${SNAKEMAKE_VER}" in \
         7.*) add-apt-repository -y ppa:deadsnakes/ppa \
              && apt-get update \
              && apt-get install -y --no-install-recommends python3.11 python3.11-venv python3.11-dev \
              && python=python3.11 ;; \
       esac \
    && apt-get update \
    && apt-get install -y --no-install-recommends apptainer squashfs-tools \
    && find / -xdev -name 'squashfuse*' -type f -delete \
    && "$python" -m venv /opt/snakemake \
    && case "${SNAKEMAKE_VER}" in \
         9.*) extra="snakemake-logger-plugin-panoptes==${PANOPTES_PLUGIN_VER}" ;; \
         7.*) extra="pulp<2.8" ;; \
         *) extra="" ;; \
       esac \
    && /opt/snakemake/bin/pip install --no-cache-dir "snakemake==${SNAKEMAKE_VER}" $extra \
    && apt-get purge -y software-properties-common gcc python3-dev && apt-get autoremove -y \
    && rm -rf /var/lib/apt/lists/* \
    && ln -s /opt/snakemake/bin/snakemake /usr/local/bin/snakemake \
    && snakemake --version \
    && case "${SNAKEMAKE_VER}" in 9.*) snakemake --help | grep -q logger-panoptes-address ;; *) snakemake --help | grep -q wms-monitor ;; esac \
    && apptainer --version

# The engine runs as the host user that owns the project directory (`--user uid:gid`), who has no
# entry in this image, and Apptainer refuses to start without one: the entrypoint adds it.
RUN chmod 666 /etc/passwd \
    && printf '%s\n' '#!/bin/sh' \
      'if ! getent passwd "$(id -u)" >/dev/null; then' \
      '  echo "macworp:x:$(id -u):$(id -g)::/tmp:/bin/bash" >> /etc/passwd' \
      'fi' \
      'exec "$@"' > /usr/local/bin/engine-entrypoint \
    && chmod 755 /usr/local/bin/engine-entrypoint
ENTRYPOINT ["/usr/local/bin/engine-entrypoint"]

# CWL through Toil (Python venv of its own). Toil starts `DockerRequirement` images through the
# runtime socket like Nextflow does (docker CLI from the base image, `DOCKER_HOST` points at the
# mount). Its image pre-pull calls `cwl-docker-extract` from the venv, so the venv is on PATH.
FROM base AS cwl

ARG TOIL_VER=9.6.0
# gcc and python3-dev build wheels that have none for the platform (psutil on arm64), removed again
RUN apt-get update \
    && apt-get install -y --no-install-recommends python3 python3-venv python3-pip nodejs gcc python3-dev \
    && python3 -m venv /opt/toil \
    && /opt/toil/bin/pip install --no-cache-dir "toil[cwl]==${TOIL_VER}" \
    && apt-get purge -y gcc python3-dev && apt-get autoremove -y \
    && rm -rf /var/lib/apt/lists/* \
    && /opt/toil/bin/toil-cwl-runner --version \
    # Toil's `--podman` mode runs `podman`; against a Podman socket that is the remote client
    && ln -s "$(command -v podman-remote)" /usr/local/bin/podman
ENV PATH=/opt/toil/bin:$PATH
