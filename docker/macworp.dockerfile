FROM mambaorg/micromamba:2-ubuntu24.04
LABEL maintainer="dirk.winkelhardt@rub.de"

# Note: One could use a basic Python image to install the worker package
# but you have to install the mandatory dependencies via the OS package manager next to the conda environment
# making the package less maintainable.

USER root
ENV DEBIAN_FRONTEND=noninteractive 
ENV TZ=Etc/UTC

# Native installs
RUN apt-get update -y \
    && apt-get install -y ca-certificates curl zip unzip openjdk-21-jre-headless software-properties-common tzdata \
    && add-apt-repository -y ppa:apptainer/ppa \
    && add-apt-repository ppa:xtradeb/apps -y \
    && apt-get update -y \
    && apt-get install -y apptainer chromium \
    && install -m 0755 -d /etc/apt/keyrings \
    && curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc \
    && chmod a+r /etc/apt/keyrings/docker.asc \
    && echo \
        "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu \
        $(. /etc/os-release && echo "$VERSION_CODENAME") stable" | \
        tee /etc/apt/sources.list.d/docker.list > /dev/null \
    && apt-get update -y \
    && apt install -y docker-ce-cli docker-ce \
    # Make sure the mambauser can run docker
    && usermod -aG docker mambauser \
    # Remove caches
    && rm -rf /var/lib/apt/lists/* 

# Nextflow install
WORKDIR /usr/local/bin
RUN curl -s https://get.nextflow.io | bash \ 
    && chmod 755 /usr/local/bin/nextflow

WORKDIR /home/mambauser
# Copy code and environment file
COPY --chown=mambauser:mambauser src/ ./src
COPY --chown=mambauser:mambauser pyproject.toml .
COPY --chown=mambauser:mambauser MANIFEST.in .
COPY --chown=mambauser:mambauser environment.yml .

# Switch to mamabauser and install the the conda environment
USER mambauser
ENV HOME=/home/mambauser
ENV PATH=$PATH:$HOME/.local/bin
ENV ENV_NAME=macworp-dev-v2
ENV BROWSER_PATH=/usr/bin/chromium
ENV NXF_VER=24.09.2-edge

RUN echo 'show_banner: false' > ~/.mambarc

# Remove unneeded dependencies python modules and extras and add GIT
RUN sed -i 's;^.*./backend.*$;;g' environment.yml \
    # Remove the optional development dependencies
    && sed -i 's;\[dev\];;' environment.yml \
    # Remove editable flag and set src directory
    && sed -i 's;-e .;.;' environment.yml \
    # Remove the optional dev and doc dependencies
    && sed -i 's;\[dev,demo\];\[demo\];' environment.yml \
    && sed -i 's;^.*requirements.txt.*$;;g' environment.yml

RUN micromamba env create -y -f environment.yml \
    && micromamba clean --all --yes

# Switch back to root and set the entrypoint
USER root

# Add root entrypoints
COPY docker/entrypoints/change_mambauser.sh /usr/local/bin/change_mambauser.sh
COPY docker/entrypoints/set_docker_socket_permissions.sh /usr/local/bin/set_docker_socket_permissions.sh
COPY docker/entrypoints/set_projects_dir_permissions.sh /usr/local/bin/set_projects_dir_permissions.sh
COPY docker/entrypoints/migration_and_seed.sh /usr/local/bin/migration_and_seed.sh

# 1. Set up mambauser to access the docker socket
# 2. Set up mambauser to access the projects directory
# 3. Change the user to mambauser
# 4. Activate the conda environment
# 5. Check if migration and seeding is needed 
# 6. Start the macworp *
ENTRYPOINT [ "/usr/local/bin/set_docker_socket_permissions.sh", "/usr/local/bin/set_projects_dir_permissions.sh", "/usr/local/bin/change_mambauser.sh", "/usr/local/bin/_entrypoint.sh", "/usr/local/bin/migration_and_seed.sh", "macworp" ]
