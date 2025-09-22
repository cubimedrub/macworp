FROM mambaorg/micromamba:2-debian13
LABEL maintainer="dirk.winkelhardt@rub.de"

# Note: One could use a basic Python image to install the backend package
# but you have to install the mandatory dependencies via the OS package manager next to the conda environment
# making the package less maintainable.

USER root
ENV PROJECTS_DIR=/projects

# Native installs
RUN apt-get update -y \
    && apt-get install -y postgresql-client-17 curl \
    # Remove caches
    && rm -rf /var/lib/apt/lists/* 

# Add root entrypoints
COPY docker/entrypoints/change_mambauser.sh /usr/local/bin/change_mambauser.sh
COPY docker/entrypoints/set_projects_dir_permissions.sh /usr/local/bin/set_projects_dir_permissions.sh

# Switch to mambauser home
WORKDIR /home/mambauser

# Copy Python modules, environment file and backend entrypoint
COPY --chown=mambauser:mambauser backend/ ./backend/
COPY --chown=mambauser:mambauser utils/ ./utils/
COPY --chown=mambauser:mambauser environment.yml .
COPY docker/entrypoints/backend.sh ./entrypoint.sh

# Switch to mamabauser and install the the conda environment
USER mambauser
ENV HOME=/home/mambauser
ENV PATH=$PATH:$HOME/.local/bin
ENV ENV_NAME=macworp

RUN echo 'show_banner: false' > ~/.mambarc

# Remove unneeded dependencies python modules and extras and add GIT
RUN sed -i 's;^.*./worker.*$;;g' environment.yml \
    && sed -i 's;^.*- nodejs=.*$;;g' environment.yml \
    && sed -i 's;yarn;git;' environment.yml \
    && sed -i 's;\[dev\];;' environment.yml \
    && sed -i 's;^.*requirements.txt.*$;;g' environment.yml

RUN micromamba env create -y -f environment.yml \
    && micromamba clean --all --yes


# Switch back to root and set the entrypoint
USER root

# 1. Set up mambauser to access the projects directory
# 2. Change the user to mambauser
# 3. Activate the conda environment
# 4. Start the backend
ENTRYPOINT [ "/usr/local/bin/set_projects_dir_permissions.sh", "/usr/local/bin/change_mambauser.sh", "/usr/local/bin/_entrypoint.sh", "/home/mambauser/entrypoint.sh" ]
