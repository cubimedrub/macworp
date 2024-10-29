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
        --move-home "-u ${NEW_MAMBA_USER_ID}" "${MAMBA_USER}" && \
    groupmod "--new-name=mambauser" \
        "-g ${NEW_MAMBA_USER_GID}" "${MAMBA_USER}" && \
    # Update the expected value of MAMBA_USER for the
    # _entrypoint.sh consistency check.
    echo "mambauser" > "/etc/arg_mamba_user" && \
    :

RUN apt-get update -y \
    && apt-get install -y postgresql-client-14

WORKDIR /home/mambauser
# Copy backend and environment.yml
COPY --chown=mambauser:mambauser backend/ ./backend/
COPY --chown=mambauser:mambauser utils/ ./utils/
COPY --chown=mambauser:mambauser environment.yml .
COPY --chown=mambauser:mambauser docker/entrypoints/backend.sh ./entrypoint.sh

USER mambauser
ENV HOME=/home/mambauser
ENV PATH=$PATH:$HOME/.local/bin
ENV ENV_NAME=macworp

RUN echo 'show_banner: false' > ~/.mambarc

ENV PATH=$PATH:$HOME/.cargo/bin

# Remove unneeded dependencies python modules and extras and add GIT
RUN sed -i 's;^.*./worker.*$;;g' environment.yml \
    && sed -i 's;^.*- nodejs=.*$;;g' environment.yml \
    && sed -i 's;yarn;git;' environment.yml \
    && sed -i 's;\[dev\];;' environment.yml

RUN micromamba env create -y -f environment.yml \
    && micromamba clean --all --yes

# ARG MAMBA_DOCKERFILE_ACTIVATE=1
# RUN pipenv install --deploy

ENTRYPOINT [ "/usr/local/bin/_entrypoint.sh", "/home/mambauser/entrypoint.sh" ]
