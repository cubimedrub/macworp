FROM continuumio/miniconda3
LABEL maintainer="dirk.winkelhardt@rub.de"

ARG USER_ID=999
ARG GROUP_ID=999

ENV USER_ID=$USER_ID
ENV GROUP_ID=$GROUP_ID

WORKDIR /home/app
# Copy backend and environment.yml
COPY backend/ ./backend/
COPY environment.yml .

RUN apt-get update -y && apt-get install -y build-essential libev-dev \
    && conda update -n base conda -c defaults \
    && groupadd -g $GROUP_ID app \
    && useradd -g $GROUP_ID -m -s /bin/bash -u $USER_ID app \
    && chown -R app:app /home/app

USER app
ENV HOME /home/app
ENV PATH $PATH:$HOME/.local/bin

# Remove the worker installation and dev-dependencies
RUN sed -i 's;^.*./worker.*$;;g' environment.yml \
    && sed -i 's;\[dev\];;' environment.yml \
    && ls -la \
    && conda env create -f environment.yml

ENTRYPOINT ["conda", "run", "-n", "nf_cloud", "python", "-m", "nf_cloud_backend" ]
