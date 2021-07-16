FROM python:3.8.8-buster

WORKDIR /app

COPY ./nf_cloud /app/nf_cloud
COPY ./nf_cloud/entrypoint.sh /app/
COPY Pipfile /app
COPY Pipfile.lock /app
COPY config.yaml /app
COPY config.production.yaml /app

RUN apt-get update -y && apt-get install -y libev-dev \
    && pip install --upgrade pip \
    && pip install pipenv \
    && pipenv install --system --skip-lock \
    && chmod 755 ./entrypoint.sh

ENV NF_CLOUD_ENV production

ENTRYPOINT [ "./entrypoint.sh" ]
