# Engine images: one workflow engine per image, so versions and privileges stay independent.
# The engine starts sibling tool containers through a mounted runtime socket, see
# docs/adr/0004-windows-strategy.md. Build one target:
#
#   docker build -f docker/engine.dockerfile --target nextflow  -t macworp-nextflow:latest  docker
#   docker build -f docker/engine.dockerfile --target snakemake -t macworp-snakemake:latest docker
#   docker build -f docker/engine.dockerfile --target cwl       -t macworp-cwl:latest       docker

# Shared by all engines: container CLIs and basic tools
FROM ubuntu:24.04 AS base

RUN apt-get update \
    && apt-get install -y --no-install-recommends bash curl ca-certificates procps git docker.io podman-remote \
    && rm -rf /var/lib/apt/lists/*

# Nextflow (needs a JRE)
FROM base AS nextflow

ARG NXF_VER=25.04.6
ENV NXF_VER=${NXF_VER} NXF_HOME=/opt/nextflow NXF_ANSI_LOG=false NXF_OPTS="-Xmx1g"

COPY --from=eclipse-temurin:21-jre /opt/java/openjdk /opt/java/openjdk
ENV JAVA_HOME=/opt/java/openjdk PATH=/opt/java/openjdk/bin:$PATH

RUN curl -fsSL https://get.nextflow.io | bash \
    && mv nextflow /usr/local/bin/nextflow \
    && chmod 755 /usr/local/bin/nextflow \
    && nextflow -version

# Snakemake (Python venv of its own) with Apptainer, which Snakemake uses to run `container:` images
FROM base AS snakemake

ARG SNAKEMAKE_VER=9.11.4
# Reports progress to the worker (Snakemake 9 removed --wms-monitor)
ARG PANOPTES_PLUGIN_VER=0.3.0
# Apptainer from its PPA: the release .deb files exist for amd64 only, the PPA also serves arm64
# (Apple silicon). Inside a container there is no loop device and usually no /dev/fuse, so the
# FUSE helper (bundled squashfuse) is removed: Apptainer then unpacks images with unsquashfs into
# a temporary sandbox. It must run as a non-root user for that, see docs/container-runtimes.md.
RUN apt-get update \
    && apt-get install -y --no-install-recommends python3 python3-venv python3-pip software-properties-common \
    && add-apt-repository -y ppa:apptainer/ppa \
    && apt-get update \
    && apt-get install -y --no-install-recommends apptainer squashfs-tools \
    && find / -xdev -name 'squashfuse*' -type f -delete \
    && apt-get purge -y software-properties-common && apt-get autoremove -y \
    && rm -rf /var/lib/apt/lists/* \
    && python3 -m venv /opt/snakemake \
    && /opt/snakemake/bin/pip install --no-cache-dir "snakemake==${SNAKEMAKE_VER}" "snakemake-logger-plugin-panoptes==${PANOPTES_PLUGIN_VER}" \
    && ln -s /opt/snakemake/bin/snakemake /usr/local/bin/snakemake \
    && snakemake --version \
    && snakemake --help | grep -q logger-panoptes-address \
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
RUN apt-get update \
    && apt-get install -y --no-install-recommends python3 python3-venv python3-pip nodejs \
    && rm -rf /var/lib/apt/lists/* \
    && python3 -m venv /opt/toil \
    && /opt/toil/bin/pip install --no-cache-dir "toil[cwl]==${TOIL_VER}" \
    && /opt/toil/bin/toil-cwl-runner --version
ENV PATH=/opt/toil/bin:$PATH
