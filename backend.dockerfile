FROM continuumio/miniconda3

WORKDIR /app

COPY ./nf_cloud_backend /app/nf_cloud_backend
COPY ./nf_cloud_backend/entrypoint.sh /app/
COPY environment.yml /app
COPY config.yaml /app
COPY config.production.yaml /app

RUN apt-get update -y && apt-get install -y build-essential libev-dev \
    && conda update -n base conda -c defaults \
    && conda env create -f environment.yml \
    && chmod +x entrypoint.sh

ENV NF_CLOUD_WEB_ENV production

ENTRYPOINT [ "bash", "-i", "./entrypoint.sh" ]
