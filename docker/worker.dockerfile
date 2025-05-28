FROM mambaorg/micromamba:1.2.0-jammy
LABEL maintainer="dirk.winkelhardt@rub.de"

# Note: One could use a basic Python image to install the backend package
# but you have to install the mandatory dependencies via the OS package manager next to the conda environment
# making the package less maintainable.

USER root

# Native installs
RUN apt-get update -y \
    && apt-get install -y ca-certificates curl zip unzip openjdk-17-jre-headless software-properties-common \
    && add-apt-repository -y ppa:apptainer/ppa \
    && apt-get update -y \
    && apt-get install -y apptainer \
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

# Add root entrypoints
COPY docker/entrypoints/change_mambauser.sh /usr/local/bin/change_mambauser.sh
COPY docker/entrypoints/set_docker_socket_permissions.sh /usr/local/bin/set_docker_socket_permissions.sh
COPY docker/entrypoints/set_projects_dir_permissions.sh /usr/local/bin/set_projects_dir_permissions.sh

# Nextflow install
WORKDIR /usr/local/bin
RUN curl -s https://get.nextflow.io | bash \ 
    && chmod 755 /usr/local/bin/nextflow

WORKDIR /home/mambauser
# Copy backend and environment.yml
COPY --chown=mambauser:mambauser worker/ ./worker/
COPY --chown=mambauser:mambauser utils/ ./utils/
COPY --chown=mambauser:mambauser environment.yml .

# Switch to mamabauser and install the the conda environment
USER mambauser
ENV HOME=/home/mambauser
ENV PATH=$PATH:$HOME/.local/bin
ENV ENV_NAME=macworp
ENV NXF_VER=24.09.2-edge

RUN echo 'show_banner: false' > ~/.mambarc

# Remove unneeded dependencies python modules and extras and add GIT
RUN sed -i 's;^.*./backend.*$;;g' environment.yml \
    && sed -i 's;^.*- nodejs=.*$;;g' environment.yml \
    && sed -i 's;yarn;git;' environment.yml \
    # Remove the optional development dependencies
    && sed -i 's;\[dev\];;' environment.yml \
    # Remove the optional dev and doc dependencies
    && sed -i 's;\[dev,demo\];\[demo\];' environment.yml \
    && sed -i 's;^.*requirements.txt.*$;;g' environment.yml

RUN micromamba env create -y -f environment.yml \
    && micromamba clean --all --yes

# Switch back to root and set the entrypoint
USER root

# 1. Set up mambauser to access the docker socket
# 2. Set up mambauser to access the projects directory
# 3. Change the user to mambauser
# 4. Activate the conda environment
# 5. Start the backend
ENTRYPOINT [ "/usr/local/bin/set_docker_socket_permissions.sh", "/usr/local/bin/set_projects_dir_permissions.sh", "/usr/local/bin/change_mambauser.sh", "/usr/local/bin/_entrypoint.sh", "python", "-m", "macworp_worker" ]
