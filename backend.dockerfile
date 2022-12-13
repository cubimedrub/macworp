FROM continuumio/miniconda3 as development
# Multistage build. First stage is only for development. Using a bind mount to provide the code.
LABEL maintainer="dirk.winkelhardt@rub.de"

ARG USER_ID=999
ARG GROUP_ID=999

ENV USER_ID=$USER_ID
ENV GROUP_ID=$GROUP_ID

WORKDIR /home/app
COPY environment.yml .

RUN apt-get update -y && apt-get install -y build-essential libev-dev postgresql-client \
    && conda update -n base conda -c defaults \
    && groupadd -g $GROUP_ID app \
    && useradd -g $GROUP_ID -m -s /bin/bash -u $USER_ID app \
    && chown -R app:app /home/app

USER app
ENV HOME /home/app
ENV PATH $PATH:$HOME/.local/bin

RUN env MAKEFLAGS="-j$(nproc)" conda env create -f environment.yml

ENTRYPOINT ["conda", "run", "--no-capture-output", "--live-stream", "-n", "nf_cloud", "/home/app/entrypoint.sh" ]


FROM development as production
# Second stage copies the the code for stand alone use.
COPY nf_cloud_backend/ ./nf_cloud_backend
COPY nf_cloud_backend/entrypoint.sh .

USER root
RUN chmod +x entrypoint.sh \
    && chown -R app:app /home/app

USER app