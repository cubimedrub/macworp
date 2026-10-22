# Engine image: Nextflow + container CLIs so the engine can start sibling tool containers
# through a mounted runtime socket. See docs/adr/0004-windows-strategy.md.
FROM eclipse-temurin:21-jre

ARG NXF_VER=25.04.6
ENV NXF_VER=${NXF_VER} NXF_HOME=/opt/nextflow NXF_ANSI_LOG=false NXF_OPTS="-Xmx1g"

RUN apt-get update \
    && apt-get install -y --no-install-recommends bash curl ca-certificates procps git docker.io podman-remote \
    && rm -rf /var/lib/apt/lists/*

RUN curl -fsSL https://get.nextflow.io | bash \
    && mv nextflow /usr/local/bin/nextflow \
    && chmod 755 /usr/local/bin/nextflow \
    && nextflow -version
