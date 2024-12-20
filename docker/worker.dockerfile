FROM mambaorg/micromamba:1.2.0-jammy
LABEL maintainer="dirk.winkelhardt@rub.de"

# Note: One could use a basic Python image to install the backend package
# but you have to install the mandatory dependencies via the OS package manager next to the conda environment
# making the package less maintainable.
# Another benefit is the use of a dedicated mamba user to adjust the UID/GID so the uploaded data is not owned by root.


ARG NEW_MAMBA_USER_ID=1000
ARG NEW_MAMBA_USER_GID=1000

USER root
RUN usermod "--login=mambauser" "--home=/home/mambauser" \
        --move-home "-u ${NEW_MAMBA_USER_ID}" "${MAMBA_USER}" \
    && groupmod "--new-name=mambauser" \
        "-g ${NEW_MAMBA_USER_GID}" "${MAMBA_USER}" \
    && echo "mambauser" > "/etc/arg_mamba_user" \
    # Install Docker
    && apt-get update -y \
    && apt-get install -y ca-certificates curl zip unzip openjdk-17-jre-headless \
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
    && usermod -aG root mambauser \
    # Remove caches
    && rm -rf /var/lib/apt/lists/* 


WORKDIR /usr/local/bin
RUN curl -s https://get.nextflow.io | bash \ 
    && chmod 755 /usr/local/bin/nextflow

WORKDIR /home/mambauser
# Copy backend and environment.yml
COPY --chown=mambauser:mambauser worker/ ./worker/
COPY --chown=mambauser:mambauser utils/ ./utils/
COPY --chown=mambauser:mambauser environment.yml .

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

ENTRYPOINT [ "/usr/local/bin/_entrypoint.sh", "python", "-m", "macworp_worker" ]
